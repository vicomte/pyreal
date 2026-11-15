//! The rows the probe collects, printed as a table and written as JSON.

use serde_json::{json, Value};

use crate::gpu::Gpu;

/// What a row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The adapter offers the feature.
    Yes,
    /// The adapter does not offer it.
    No,
    /// A workload ran and its result checked out.
    Pass,
    /// A workload ran and failed, or could not be set up.
    Fail,
    /// A workload was not run, because what it needs is missing.
    Skip,
    /// A value: a limit, a size, a format's capabilities.
    Info,
}

impl Status {
    fn word(self) -> &'static str {
        match self {
            Status::Yes => "yes",
            Status::No => "no",
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Skip => "skip",
            Status::Info => "",
        }
    }
}

/// One line of the table.
#[derive(Debug, Clone)]
pub struct Row {
    pub section: &'static str,
    pub item: String,
    pub status: Status,
    pub detail: String,
}

/// One timed workload: CPU time to record and submit, and GPU time from timestamps.
#[derive(Debug, Clone)]
pub struct Timing {
    pub section: &'static str,
    pub name: String,
    pub cpu_ms: Option<f64>,
    pub gpu_ms: Option<f64>,
    pub note: String,
}

/// Everything the probe found.
#[derive(Debug)]
pub struct Report {
    pub rows: Vec<Row>,
    pub timings: Vec<Timing>,
    header: Vec<String>,
}

impl Report {
    pub fn new(gpu: &Gpu) -> Self {
        let info = gpu.adapter.get_info();
        let header = vec![
            format!(
                "adapter   {} ({:?}, {:?})",
                info.name, info.backend, info.device_type
            ),
            format!("driver    {} {}", info.driver, info.driver_info),
            format!(
                "subgroup  {}..{} lanes",
                info.subgroup_min_size, info.subgroup_max_size
            ),
            format!("flags     {:?}", gpu.instance_flags),
        ];
        Self {
            rows: Vec::new(),
            timings: Vec::new(),
            header,
        }
    }

    pub fn row(
        &mut self,
        section: &'static str,
        item: impl Into<String>,
        status: Status,
        detail: impl Into<String>,
    ) {
        self.rows.push(Row {
            section,
            item: item.into(),
            status,
            detail: detail.into(),
        });
    }

    /// A feature the adapter does or does not offer.
    pub fn feature(&mut self, section: &'static str, gpu: &Gpu, f: wgpu::Features) -> bool {
        let have = gpu.features.contains(f);
        self.row(
            section,
            f.to_string(),
            if have { Status::Yes } else { Status::No },
            "",
        );
        have
    }

    /// A workload's verdict: `Ok(detail)` passes, `Err(why)` fails.
    pub fn result(
        &mut self,
        section: &'static str,
        item: impl Into<String>,
        r: Result<String, String>,
    ) {
        match r {
            Ok(d) => self.row(section, item, Status::Pass, d),
            Err(e) => self.row(section, item, Status::Fail, e),
        }
    }

    pub fn time(
        &mut self,
        section: &'static str,
        name: impl Into<String>,
        cpu_ms: Option<f64>,
        gpu_ms: Option<f64>,
        note: impl Into<String>,
    ) {
        self.timings.push(Timing {
            section,
            name: name.into(),
            cpu_ms,
            gpu_ms,
            note: note.into(),
        });
    }

    pub fn failures(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.status == Status::Fail)
            .count()
    }

    pub fn print(&self) {
        for h in &self.header {
            println!("{h}");
        }
        let w = self
            .rows
            .iter()
            .map(|r| r.item.len())
            .max()
            .unwrap_or(0)
            .min(64);
        let mut section = "";
        for r in &self.rows {
            if r.section != section {
                section = r.section;
                println!("\n== {section}");
            }
            println!("  {:<w$}  {:<4}  {}", r.item, r.status.word(), r.detail);
        }
        if !self.timings.is_empty() {
            println!("\n== timings (median of the runs; ms)");
            let w = self
                .timings
                .iter()
                .map(|t| t.name.len())
                .max()
                .unwrap_or(0)
                .min(64);
            println!("  {:<w$}  {:>9}  {:>9}  note", "workload", "cpu", "gpu");
            for t in &self.timings {
                println!(
                    "  {:<w$}  {:>9}  {:>9}  {}",
                    t.name,
                    ms(t.cpu_ms),
                    ms(t.gpu_ms),
                    t.note
                );
            }
        }
    }

    pub fn json(&self, gpu: &Gpu) -> String {
        let info = gpu.adapter.get_info();
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|r| {
                json!({
                    "section": r.section,
                    "item": r.item,
                    "status": format!("{:?}", r.status).to_lowercase(),
                    "detail": r.detail,
                })
            })
            .collect();
        let timings: Vec<Value> = self
            .timings
            .iter()
            .map(|t| {
                json!({
                    "section": t.section,
                    "name": t.name,
                    "cpu_ms": t.cpu_ms,
                    "gpu_ms": t.gpu_ms,
                    "note": t.note,
                })
            })
            .collect();
        let features: Vec<&str> = gpu.features.iter_names().map(|(n, _)| n).collect();
        let v = json!({
            "adapter": {
                "name": info.name,
                "backend": format!("{:?}", info.backend),
                "device_type": format!("{:?}", info.device_type),
                "driver": info.driver,
                "driver_info": info.driver_info,
                "subgroup_min_size": info.subgroup_min_size,
                "subgroup_max_size": info.subgroup_max_size,
            },
            "instance_flags": format!("{:?}", gpu.instance_flags),
            "features": features,
            "limits": format!("{:#?}", gpu.limits),
            "rows": rows,
            "timings": timings,
        });
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }
}

fn ms(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_owned(), |v| format!("{v:.3}"))
}
