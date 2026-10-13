// Native Rust benchmarking and load testing tool for WhalePrep.
// 100% Rust, zero Python dependency.
//
// Usage:
//   cargo run --release --bin bench -- [BASE_URL] [CONCURRENCY] [REQUESTS]
// Example:
//   cargo run --release --bin bench -- https://whaleprep.oiupoyt.space 10 100
//   cargo run --release --bin bench -- http://127.0.0.1:8080 20 200

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct Stats {
    name: String,
    durations_ms: Vec<f64>,
    errors: usize,
}

impl Stats {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            durations_ms: Vec::new(),
            errors: 0,
        }
    }

    fn print(&mut self, total_elapsed: Duration) {
        if self.durations_ms.is_empty() {
            println!("  {:<22} No successful samples ({} errors)", self.name, self.errors);
            return;
        }
        self.durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let count = self.durations_ms.len();
        let p50 = self.durations_ms[count * 50 / 100];
        let p90 = self.durations_ms[count * 90 / 100];
        let p95 = self.durations_ms[count * 95 / 100];
        let p99 = self.durations_ms[(count * 99 / 100).min(count - 1)];
        let _min = self.durations_ms[0];
        let max = self.durations_ms[count - 1];
        let avg = self.durations_ms.iter().sum::<f64>() / count as f64;
        let rps = count as f64 / total_elapsed.as_secs_f64();

        println!(
            "  {:<20} n={:<4} avg={:>6.2}ms p50={:>6.2}ms p90={:>6.2}ms p95={:>6.2}ms p99={:>6.2}ms max={:>6.2}ms ({:>5.0} req/s, {} err)",
            self.name, count, avg, p50, p90, p95, p99, max, rps, self.errors
        );
    }
}

async fn make_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) WhalePrep-RustBench/1.0")
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).map(|s| s.as_str()).unwrap_or("https://whaleprep.oiupoyt.space").trim_end_matches('/');
    let concurrency: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let requests: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100);

    println!("============================================================");
    println!(" WhalePrep Native Rust Benchmark");
    println!(" Target URL:   {}", base);
    println!(" Concurrency:  {} workers", concurrency);
    println!(" Requests:     {} per endpoint", requests);
    println!("============================================================\n");

    // Test connectivity first
    let health_client = make_client().await;
    let t0 = Instant::now();
    let resp = health_client.get(format!("{}/", base)).send().await;
    match resp {
        Ok(r) => {
            println!("Target is ONLINE (HTTP {}, ping: {:.2}ms)\n", r.status(), t0.elapsed().as_secs_f64() * 1000.0);
        }
        Err(e) => {
            eprintln!("ERROR: Could not connect to target {}: {}\n", base, e);
            std::process::exit(1);
        }
    }

    // 1. READ BENCHMARKS
    println!("--- [1] READ ENDPOINTS CONCURRENCY LOAD ---");
    let endpoints = vec![
        ("Landing (/) ", "/"),
        ("CSS (static) ", "/static/style.css"),
        ("Builder (/build) ", "/build"),
        ("Dashboard ", "/dashboard"),
        ("History ", "/history"),
    ];

    for (label, path) in endpoints {
        let mut stats = Stats::new(label);
        let url = format!("{}{}", base, path);
        let error_count = Arc::new(AtomicUsize::new(0));

        let t_total = Instant::now();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::with_capacity(requests);

        for _ in 0..requests {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let url_clone = url.clone();
            let err_cnt = error_count.clone();

            let handle = tokio::spawn(async move {
                let client = make_client().await;
                let t_req = Instant::now();
                let res = client.get(&url_clone).send().await;
                let ms = t_req.elapsed().as_secs_f64() * 1000.0;
                drop(permit);
                match res {
                    Ok(r) if r.status().is_success() || r.status().is_redirection() => Some(ms),
                    _ => {
                        err_cnt.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            });
            handles.push(handle);
        }

        for h in handles {
            if let Ok(Some(ms)) = h.await {
                stats.durations_ms.push(ms);
            }
        }
        stats.errors = error_count.load(Ordering::Relaxed);
        stats.print(t_total.elapsed());
    }

    // 2. WRITE BENCHMARK (Simulated Full Student Lifecycle)
    println!("\n--- [2] STUDENT LIFECYCLE (WRITE & GRADING PIPELINE) ---");
    let student_workers = (concurrency / 2).max(2).min(10);
    println!("Simulating {} concurrent students (Login -> Build Paper -> Attempt -> Autosave -> Submit):", student_workers);

    let t_write_total = Instant::now();
    let mut login_stats = Stats::new("Student Login");
    let mut build_stats = Stats::new("Paper Build");
    let mut attempt_stats = Stats::new("Start Attempt");
    let mut autosave_stats = Stats::new("Autosave (Write)");
    let mut submit_stats = Stats::new("Finalize & Grade");

    let mut student_handles = Vec::new();
    for i in 0..student_workers {
        let base_url = base.to_string();
        let handle = tokio::spawn(async move {
            let client = make_client().await;
            let user_email = format!("rustbench{}@example.com", i);
            let user_name = format!("RustBench{}", i);

            // 1. Sign in
            let t0 = Instant::now();
            let res = client
                .post(format!("{}/student", base_url))
                .form(&[("name", user_name), ("email", user_email), ("next", "/build".to_string())])
                .send()
                .await;
            let login_ms = t0.elapsed().as_secs_f64() * 1000.0;
            if res.is_err() {
                return (None, None, None, vec![], None);
            }

            // 2. Build paper
            let t1 = Instant::now();
            let res = client
                .post(format!("{}/build", base_url))
                .form(&[
                    ("chapters", "1,2,3,4,7,8"),
                    ("count", "12"),
                    ("duration", "24"),
                    ("unseen_only", "on"),
                ])
                .send()
                .await;
            let build_ms = t1.elapsed().as_secs_f64() * 1000.0;
            let paper_url = match res {
                Ok(r) => r.url().to_string(),
                Err(_) => return (Some(login_ms), None, None, vec![], None),
            };
            let paper_id = paper_url.rsplit('/').next().unwrap_or("").split('?').next().unwrap_or("").to_string();
            if paper_id.is_empty() {
                return (Some(login_ms), Some(build_ms), None, vec![], None);
            }

            // 3. Start attempt
            let t2 = Instant::now();
            let res = client
                .post(format!("{}/attempt/{}", base_url, paper_id))
                .send()
                .await;
            let attempt_ms = t2.elapsed().as_secs_f64() * 1000.0;
            let attempt_url = match res {
                Ok(r) => r.url().to_string(),
                Err(_) => return (Some(login_ms), Some(build_ms), None, vec![], None),
            };
            let attempt_id = attempt_url.split("run=").nth(1).unwrap_or("").to_string();
            if attempt_id.is_empty() {
                return (Some(login_ms), Some(build_ms), Some(attempt_ms), vec![], None);
            }

            // Fetch question IDs from state API
            let state_res = client
                .get(format!("{}/api/attempt/{}/state", base_url, attempt_id))
                .send()
                .await;
            let mut qids = Vec::new();
            if let Ok(r) = state_res {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    if let Some(answers) = json["answers"].as_array() {
                        for a in answers {
                            if let Some(qid) = a["question_id"].as_i64() {
                                qids.push(qid);
                            }
                        }
                    }
                }
            }

            // 4. Autosave answers
            let mut save_times = Vec::new();
            for qid in qids {
                let payload = serde_json::json!({
                    "answers": [{
                        "question_id": qid,
                        "selected": "A",
                        "marked_review": false,
                        "time_secs": 15
                    }]
                });
                let t_save = Instant::now();
                let s_res = client
                    .post(format!("{}/attempt/{}/answer", base_url, attempt_id))
                    .json(&payload)
                    .send()
                    .await;
                if s_res.is_ok() {
                    save_times.push(t_save.elapsed().as_secs_f64() * 1000.0);
                }
            }

            // 5. Submit paper
            let t_sub = Instant::now();
            let sub_res = client
                .post(format!("{}/attempt/{}/submit", base_url, attempt_id))
                .send()
                .await;
            let submit_ms = t_sub.elapsed().as_secs_f64() * 1000.0;
            let sub_ok = sub_res.is_ok();

            (
                Some(login_ms),
                Some(build_ms),
                Some(attempt_ms),
                save_times,
                if sub_ok { Some(submit_ms) } else { None },
            )
        });
        student_handles.push(handle);
    }

    for h in student_handles {
        if let Ok((login, build, attempt, saves, submit)) = h.await {
            if let Some(ms) = login { login_stats.durations_ms.push(ms); } else { login_stats.errors += 1; }
            if let Some(ms) = build { build_stats.durations_ms.push(ms); } else { build_stats.errors += 1; }
            if let Some(ms) = attempt { attempt_stats.durations_ms.push(ms); } else { attempt_stats.errors += 1; }
            for s in saves { autosave_stats.durations_ms.push(s); }
            if let Some(ms) = submit { submit_stats.durations_ms.push(ms); } else { submit_stats.errors += 1; }
        }
    }

    login_stats.print(t_write_total.elapsed());
    build_stats.print(t_write_total.elapsed());
    attempt_stats.print(t_write_total.elapsed());
    autosave_stats.print(t_write_total.elapsed());
    submit_stats.print(t_write_total.elapsed());

    println!("\n============================================================");
    println!(" Benchmark complete in {:.2}s. 100% Rust tooling verified.", t_write_total.elapsed().as_secs_f64());
    println!("============================================================");

    Ok(())
}
