//! The frame graph: passes declared with the resources they read and write, checked once, then
//! run in their declared order each frame, every pass carrying a begin and end timestamp.
//!
//! wgpu places the barriers between passes itself, from each pass's bindings; the declarations
//! are what the graph is checked against (every read written earlier or imported) and what the
//! timings are named by. Timestamps are resolved in a submission of their own: on Metal a resolve
//! in the submission that wrote them reads zero for the last pass (measured by `pyreal-gpu-probe`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

/// What a pass encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    Compute,
    Render,
}

/// One pass: its name, kind and the named resources it reads and writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassDecl {
    pub name: &'static str,
    pub kind: PassKind,
    pub reads: Vec<&'static str>,
    pub writes: Vec<&'static str>,
}

impl PassDecl {
    #[must_use]
    pub fn new(name: &'static str, kind: PassKind) -> Self {
        Self {
            name,
            kind,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    #[must_use]
    pub fn reads(mut self, r: &[&'static str]) -> Self {
        self.reads.extend_from_slice(r);
        self
    }

    #[must_use]
    pub fn writes(mut self, w: &[&'static str]) -> Self {
        self.writes.extend_from_slice(w);
        self
    }
}

/// A graph that does not hold together.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    #[error("two passes are named {0}")]
    DuplicatePass(&'static str),
    #[error("pass {pass} reads {resource}, which nothing before it writes and nothing imports")]
    Unwritten {
        pass: &'static str,
        resource: &'static str,
    },
    #[error("{0} is written but never read and not exported")]
    Unread(&'static str),
}

/// The declared passes, in order.
#[derive(Debug, Clone)]
pub struct GraphDecl {
    pub passes: Vec<PassDecl>,
    /// Resources that come from outside the frame (the scene's pools, the material table).
    pub imported: Vec<&'static str>,
    /// Resources that leave the frame (the output).
    pub exported: Vec<&'static str>,
}

impl GraphDecl {
    /// Checks that the passes hold together: unique names, every read written by an earlier pass
    /// or imported, every write read later or exported.
    ///
    /// # Errors
    /// The first problem found.
    pub fn validate(&self) -> Result<(), GraphError> {
        let mut seen: Vec<&'static str> = Vec::new();
        let mut written: Vec<&'static str> = self.imported.clone();
        for p in &self.passes {
            if seen.contains(&p.name) {
                return Err(GraphError::DuplicatePass(p.name));
            }
            seen.push(p.name);
            for r in &p.reads {
                if !written.contains(r) {
                    return Err(GraphError::Unwritten {
                        pass: p.name,
                        resource: r,
                    });
                }
            }
            written.extend_from_slice(&p.writes);
        }
        for (k, p) in self.passes.iter().enumerate() {
            for w in &p.writes {
                let later = self.passes[k + 1..].iter().any(|q| q.reads.contains(w));
                if !later && !self.exported.contains(w) {
                    return Err(GraphError::Unread(w));
                }
            }
        }
        Ok(())
    }
}

/// One pass's GPU time in the last frame that has been read back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassTiming {
    pub name: &'static str,
    pub ms: f64,
}

/// Where the timed frame is: recorded, its work done, its timestamps being read.
#[derive(Debug)]
enum TimerState {
    /// The next frame records timestamps.
    Idle,
    /// A frame wrote them; the flag is raised when its work is done.
    Recorded(Arc<AtomicBool>),
    /// They were resolved; the channel says when the readback is mapped.
    Mapping(mpsc::Receiver<bool>),
}

/// The timestamp queries and their readback. One frame is timed at a time; frames recorded
/// while its readback is outstanding carry no timestamps.
#[derive(Debug)]
struct Timer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period_ns: f64,
    state: TimerState,
    /// The frame being recorded writes timestamps.
    recording: bool,
}

/// The graph, ready to record frames.
#[derive(Debug)]
pub struct FrameGraph {
    decl: GraphDecl,
    timer: Option<Timer>,
    timings: Vec<PassTiming>,
    /// From the first pass's start to the last pass's end: the frame's GPU time, which the
    /// passes' own times overstate where the GPU overlaps them.
    span_ms: f64,
    /// Frames whose timings have been read back.
    timed: u64,
}

impl FrameGraph {
    /// # Errors
    /// The declaration does not validate.
    pub fn new(gpu: &crate::gpu::Gpu, decl: GraphDecl) -> Result<Self, GraphError> {
        decl.validate()?;
        let count = u32::try_from(decl.passes.len() * 2).unwrap_or(0);
        let timer = gpu.has(wgpu::Features::TIMESTAMP_QUERY).then(|| {
            let bytes = u64::from(count) * 8;
            Timer {
                set: gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("pass timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count,
                }),
                resolve: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("timestamp resolve"),
                    size: bytes,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("timestamp readback"),
                    size: bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                period_ns: f64::from(gpu.queue.get_timestamp_period()),
                state: TimerState::Idle,
                recording: false,
            }
        });
        Ok(Self {
            decl,
            timer,
            timings: Vec::new(),
            span_ms: 0.0,
            timed: 0,
        })
    }

    #[must_use]
    pub fn decl(&self) -> &GraphDecl {
        &self.decl
    }

    /// Starts recording a frame into `encoder`; it is timed when no earlier timing is pending.
    pub fn begin<'a>(&'a mut self, encoder: &'a mut wgpu::CommandEncoder) -> FrameRecorder<'a> {
        if let Some(t) = self.timer.as_mut() {
            t.recording = matches!(t.state, TimerState::Idle);
        }
        FrameRecorder {
            graph: self,
            encoder,
            next: 0,
        }
    }

    /// After the frame's submission: a timed frame raises a flag once its work is done.
    pub fn submitted(&mut self, gpu: &crate::gpu::Gpu) {
        let Some(t) = self.timer.as_mut() else {
            return;
        };
        if !std::mem::take(&mut t.recording) {
            return;
        }
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        gpu.queue
            .on_submitted_work_done(move || flag.store(true, Ordering::Release));
        t.state = TimerState::Recorded(done);
    }

    /// Moves the timed frame along: once its work is done, its timestamps are resolved in a
    /// submission of their own and read back. `wait` blocks until the timings are in.
    pub fn collect(&mut self, gpu: &crate::gpu::Gpu, wait: bool) {
        let count = u32::try_from(self.decl.passes.len() * 2).unwrap_or(0);
        let Some(t) = self.timer.as_mut() else {
            return;
        };
        let rx = loop {
            if wait {
                gpu.wait();
            } else {
                let _ = gpu.device.poll(wgpu::PollType::Poll);
            }
            match &t.state {
                TimerState::Idle => return,
                TimerState::Recorded(done) => {
                    if !done.load(Ordering::Acquire) {
                        if wait {
                            continue;
                        }
                        return;
                    }
                    let mut enc = gpu.device.create_command_encoder(&Default::default());
                    enc.resolve_query_set(&t.set, 0..count, &t.resolve, 0);
                    enc.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, u64::from(count) * 8);
                    gpu.queue.submit([enc.finish()]);
                    let (tx, rx) = mpsc::channel();
                    t.readback
                        .slice(..)
                        .map_async(wgpu::MapMode::Read, move |r| {
                            let _ = tx.send(r.is_ok());
                        });
                    t.state = TimerState::Mapping(rx);
                }
                TimerState::Mapping(rx) => match rx.try_recv() {
                    Ok(ok) => break ok,
                    Err(mpsc::TryRecvError::Empty) if wait => {}
                    Err(mpsc::TryRecvError::Empty) => return,
                    Err(mpsc::TryRecvError::Disconnected) => break false,
                },
            }
        };
        t.state = TimerState::Idle;
        if !rx {
            return;
        }
        let ticks: Vec<u64> = t
            .readback
            .slice(..)
            .get_mapped_range()
            .map(|view| {
                view.as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| u64::from_le_bytes(*c))
                    .collect()
            })
            .unwrap_or_default();
        t.readback.unmap();
        #[allow(clippy::cast_precision_loss)]
        let ms = |a: u64, b: u64| b.saturating_sub(a) as f64 * t.period_ns / 1e6;
        let pairs = ticks.as_chunks::<2>().0;
        let first = pairs.iter().map(|p| p[0]).filter(|&t| t > 0).min();
        let last = pairs.iter().map(|p| p[1]).max();
        self.timed += 1;
        self.span_ms = match (first, last) {
            (Some(a), Some(b)) => ms(a, b),
            _ => 0.0,
        };
        self.timings = self
            .decl
            .passes
            .iter()
            .zip(ticks.as_chunks::<2>().0)
            .map(|(p, pair)| PassTiming {
                name: p.name,
                ms: ms(pair[0], pair[1]),
            })
            .collect();
    }

    /// The last timings read back, one per pass in order; empty without timestamp queries.
    #[must_use]
    pub fn timings(&self) -> &[PassTiming] {
        &self.timings
    }

    /// How many frames' timings have been read back.
    #[must_use]
    pub fn timed(&self) -> u64 {
        self.timed
    }

    /// The last frame's GPU time from its first pass's start to its last pass's end, ms.
    #[must_use]
    pub fn span_ms(&self) -> f64 {
        self.span_ms
    }
}

/// One frame's recording, pass by pass in the declared order.
#[derive(Debug)]
pub struct FrameRecorder<'a> {
    graph: &'a FrameGraph,
    encoder: &'a mut wgpu::CommandEncoder,
    next: usize,
}

impl<'a> FrameRecorder<'a> {
    /// The pass named `name`, which must be the next one declared.
    fn step(&mut self, name: &'static str, kind: PassKind) -> u32 {
        let decl = &self.graph.decl.passes;
        let at = self.next;
        assert!(
            decl.get(at)
                .is_some_and(|p| p.name == name && p.kind == kind),
            "frame graph: {name} recorded out of order (expected {:?})",
            decl.get(at).map(|p| p.name)
        );
        self.next += 1;
        u32::try_from(at).unwrap_or(0)
    }

    /// The timer, when this frame is timed.
    fn timer(&self) -> Option<&'a Timer> {
        let graph: &'a FrameGraph = self.graph;
        graph.timer.as_ref().filter(|t| t.recording)
    }

    /// The encoder, for copies between passes.
    pub fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.encoder
    }

    /// Submits what is recorded so far and goes on in a fresh encoder: for work that needs a
    /// submission of its own between passes (MetalFX's).
    pub fn flush(&mut self, gpu: &crate::gpu::Gpu) {
        let done = std::mem::replace(
            self.encoder,
            gpu.device.create_command_encoder(&Default::default()),
        );
        gpu.queue.submit([done.finish()]);
    }

    pub fn compute(&mut self, name: &'static str) -> wgpu::ComputePass<'_> {
        let at = self.step(name, PassKind::Compute);
        let timestamp_writes = self.timer().map(|t| wgpu::ComputePassTimestampWrites {
            query_set: &t.set,
            beginning_of_pass_write_index: Some(at * 2),
            end_of_pass_write_index: Some(at * 2 + 1),
        });
        self.encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(name),
                timestamp_writes,
            })
    }

    pub fn render(
        &mut self,
        name: &'static str,
        color: &[Option<wgpu::RenderPassColorAttachment<'_>>],
        depth: Option<wgpu::RenderPassDepthStencilAttachment<'_>>,
    ) -> wgpu::RenderPass<'_> {
        let at = self.step(name, PassKind::Render);
        let timestamp_writes = self.timer().map(|t| wgpu::RenderPassTimestampWrites {
            query_set: &t.set,
            beginning_of_pass_write_index: Some(at * 2),
            end_of_pass_write_index: Some(at * 2 + 1),
        });
        self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(name),
            color_attachments: color,
            depth_stencil_attachment: depth,
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Ends the frame; every declared pass must have been recorded.
    pub fn finish(self) {
        assert_eq!(
            self.next,
            self.graph.decl.passes.len(),
            "frame graph: passes left unrecorded"
        );
    }
}
