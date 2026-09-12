#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = crap_go::coverprofile::parse_coverprofile_bytes(data);
});
