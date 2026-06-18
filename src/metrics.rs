use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Per-stage performance metrics, accumulated globally during a run and
/// written out as JSON at the end so benchmark tooling can diff runs.
#[derive(Default, Clone)]
struct StageStat {
    total: Duration,
    count: u64,
}

#[derive(Default)]
struct Registry {
    stages: BTreeMap<&'static str, StageStat>,
    counters: BTreeMap<&'static str, u64>,
    started: Option<Instant>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// Marks the start of the run; total_wall_s is measured from here.
pub fn init() {
    registry().lock().unwrap().started = Some(Instant::now());
}

/// Records one timed invocation of a stage.
pub fn record(stage: &'static str, elapsed: Duration) {
    let mut reg = registry().lock().unwrap();
    let stat = reg.stages.entry(stage).or_default();
    stat.total += elapsed;
    stat.count += 1;
}

/// Times a closure and records it under the given stage name.
pub fn time<T>(stage: &'static str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    record(stage, start.elapsed());
    out
}

/// Increments a named counter (e.g. frames decoded/written).
pub fn inc(counter: &'static str, by: u64) {
    let mut reg = registry().lock().unwrap();
    *reg.counters.entry(counter).or_insert(0) += by;
}

fn render_json(reg: &Registry) -> String {
    let wall_s = reg
        .started
        .map(|s| s.elapsed().as_secs_f64())
        .unwrap_or(0.0);

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema\": 1,\n");
    out.push_str(&format!("  \"total_wall_s\": {:.6},\n", wall_s));

    out.push_str("  \"counters\": {\n");
    let counter_lines: Vec<String> = reg
        .counters
        .iter()
        .map(|(name, value)| format!("    \"{}\": {}", name, value))
        .collect();
    out.push_str(&counter_lines.join(",\n"));
    out.push_str("\n  },\n");

    out.push_str("  \"stages\": {\n");
    let stage_lines: Vec<String> = reg
        .stages
        .iter()
        .map(|(name, stat)| {
            let total_s = stat.total.as_secs_f64();
            let mean_ms = if stat.count > 0 {
                total_s * 1000.0 / stat.count as f64
            } else {
                0.0
            };
            format!(
                "    \"{}\": {{ \"total_s\": {:.6}, \"count\": {}, \"mean_ms\": {:.3} }}",
                name, total_s, stat.count, mean_ms
            )
        })
        .collect();
    out.push_str(&stage_lines.join(",\n"));
    out.push_str("\n  }\n");
    out.push_str("}\n");
    out
}

fn render_summary(reg: &Registry) -> String {
    let wall_s = reg
        .started
        .map(|s| s.elapsed().as_secs_f64())
        .unwrap_or(0.0);

    let mut out = String::new();
    out.push_str("==== land2port performance summary ====\n");
    out.push_str(&format!("total wall time: {:.2}s\n", wall_s));
    for (name, value) in &reg.counters {
        out.push_str(&format!("{}: {}\n", name, value));
    }
    if let Some(frames) = reg.counters.get("frames_written") {
        if wall_s > 0.0 && *frames > 0 {
            out.push_str(&format!(
                "effective throughput: {:.2} fps\n",
                *frames as f64 / wall_s
            ));
        }
    }
    out.push_str(&format!(
        "{:<18} {:>10} {:>8} {:>10} {:>7}\n",
        "stage", "total_s", "count", "mean_ms", "%wall"
    ));
    for (name, stat) in &reg.stages {
        let total_s = stat.total.as_secs_f64();
        let mean_ms = if stat.count > 0 {
            total_s * 1000.0 / stat.count as f64
        } else {
            0.0
        };
        let pct = if wall_s > 0.0 {
            total_s / wall_s * 100.0
        } else {
            0.0
        };
        out.push_str(&format!(
            "{:<18} {:>10.2} {:>8} {:>10.2} {:>6.1}%\n",
            name, total_s, stat.count, mean_ms, pct
        ));
    }
    out.push_str("=======================================");
    out
}

/// Prints the human-readable summary to stdout and writes the JSON report
/// to each of the given paths (fsynced so GCS FUSE flushes before exit).
pub fn write_report(paths: &[&str]) -> Result<()> {
    let (json, summary) = {
        let reg = registry().lock().unwrap();
        (render_json(&reg), render_summary(&reg))
    };
    println!("{}", summary);

    for path in paths {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Creating metrics directory {}", parent.display()))?;
        }
        let mut file = fs::File::create(path)
            .with_context(|| format!("Creating metrics file {}", path))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("Writing metrics file {}", path))?;
        file.sync_all()
            .with_context(|| format!("Fsyncing metrics file {}", path))?;
        println!("Metrics written to: {}", path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_accumulates_total_and_count() {
        record("test_stage_record", Duration::from_millis(10));
        record("test_stage_record", Duration::from_millis(30));

        let reg = registry().lock().unwrap();
        let stat = reg.stages.get("test_stage_record").unwrap();
        assert_eq!(stat.count, 2);
        assert!((stat.total.as_secs_f64() - 0.040).abs() < 0.001);
    }

    #[test]
    fn test_time_returns_closure_result() {
        let result = time("test_stage_time", || 41 + 1);
        assert_eq!(result, 42);

        let reg = registry().lock().unwrap();
        assert_eq!(reg.stages.get("test_stage_time").unwrap().count, 1);
    }

    #[test]
    fn test_inc_counter() {
        inc("test_counter_inc", 3);
        inc("test_counter_inc", 4);

        let reg = registry().lock().unwrap();
        assert_eq!(*reg.counters.get("test_counter_inc").unwrap(), 7);
    }

    #[test]
    fn test_json_format_is_parseable_by_bench_scripts() {
        let mut reg = Registry::default();
        reg.stages.insert(
            "detect",
            StageStat {
                total: Duration::from_millis(1500),
                count: 3,
            },
        );
        reg.counters.insert("frames_written", 3);

        let json = render_json(&reg);
        // The bench compare script greps these exact shapes; lock them in.
        assert!(json.contains("\"schema\": 1"));
        assert!(json.contains("\"frames_written\": 3"));
        assert!(
            json.contains("\"detect\": { \"total_s\": 1.500000, \"count\": 3, \"mean_ms\": 500.000 }")
        );
    }
}
