//! Benchmark of diff using the hyperAST, compared against https://github.com/GumTreeDiff/gumtree

//! algorithm: gumtree zs changedistiller rted
//! implementation: gumtree (gumtree, gumtreesimple)

//! validity: baseline gumtree, identity comparison mappings and edit scripts
//! performances: baseline gumtree, time/memory
//! code: repository (reuse ASE repositories and add some code so that gumtree works on whole commits ) / files (reuse gumtree dataset)

//! scenario #1: buggy/fixed
//! scenario #2: consecutive commits
//! scenario #2: quadratic commits ? consequence of usage ? related to precision of diff (because if we do not loose information (in result) we should get consitent results)

//! RQ 1: validity: is our implementation computing the same edit scripts that gumtree ?
//! RQ 2: performances: how our performances compare for the task of computing edit scripts on consecutive commits ? on a set of buggy/fixed files ?
//! RQ 3: scaling: what is the maximum number of commits that can be incremetally processed while staying in RAM ?
//!                what is the maximum size of the window where we can compute all combination of edit scripts ?
#[cfg(test)]
mod random_sample_diff;
#[cfg(test)]
mod swap_diff;
// #[cfg(test)]
pub mod buggy_fixed;
pub mod window_combination;
// #[cfg(test)]
// pub mod bin::window_combination;
pub mod common;
pub mod cross_repo;
pub mod diff_output;
pub mod memory_tracker;
pub mod other_tools;
pub mod postprocess;
pub mod preprocess;

// HyperAST Statistical Utilities
//
// This crate provides various statistical utilities for analyzing
// and comparing performance metrics in the HyperAST project.

/// Utilities for statistical analysis of benchmark results
pub mod stats_utils;

// Re-export all public items from stats_utils for convenience
pub use stats_utils::*;

use std::{env, fs, io, path, time};

pub fn tempfile() -> io::Result<(path::PathBuf, fs::File)> {
    let mut path = env::temp_dir();
    let file_name = time::SystemTime::UNIX_EPOCH;
    path.push(file_name.elapsed().unwrap().as_nanos().to_string());
    let file = fs::File::create(&path)?;
    Ok((path, file))
}

pub fn with_profiling<F: Fn()>(out: &path::Path, f: F) {
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&[
            // "libc",
            "libgcc", "pthread", "vdso",
        ])
        .build()
        .unwrap();
    f();
    match guard.report().build() {
        Ok(report) => {
            let mut file = fs::File::create(out).unwrap();
            let profile = report.pprof().unwrap();
            use pprof::protos::Message;
            let mut content = Vec::new();
            profile.encode(&mut content).unwrap();
            use io::Write;
            file.write_all(&content).unwrap();
        }
        Err(_) => {}
    };
}

pub fn setup_env_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .format(|buf, record| {
            use std::io::Write;
            if record.level().to_level_filter() > log::LevelFilter::Debug {
                writeln!(buf, "{}", record.args())
            } else {
                writeln!(
                    buf,
                    "[{} {}] {}",
                    buf.timestamp_millis(),
                    record.level(),
                    record.args()
                )
            }
        })
        .init();
}
