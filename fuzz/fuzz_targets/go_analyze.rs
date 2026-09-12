#![no_main]

use crap_core::Metric;
use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = crap_go::analyze_source(Path::new("fuzz.go"), &source, Metric::Cyclomatic);
});
