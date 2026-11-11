//! Placeholder binary so the crate has a `main` on every host. The real PID 1
//! binary is compiled for Linux musl targets only (see plan.md, phase 2); on a
//! developer host this prints the boot plan it would execute.

fn main() {
    println!(
        "guest-init (protocol v{})\n\nboot plan:\n{}",
        guest_init::PROTOCOL_VERSION,
        guest_init::boot_plan_text()
    );
}
