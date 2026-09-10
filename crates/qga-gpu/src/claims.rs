//! Claim banner for every QGA executable.
//!
//! Renderer output is a **Software fact**. It is not a proof of OP1–OP6,
//! inner_cone mosaic, or qga-app cosmos.

pub const CLAIM: &str = "Software fact";

/// Print `claims=Software fact  not_a_proof_of=...` so a binary cannot
/// silently look like a theorem.
pub fn print_claim_banner(not_a_proof_of: &str) {
    println!("claims={CLAIM}  not_a_proof_of={not_a_proof_of}");
}

pub fn claim_line(not_a_proof_of: &str) -> String {
    format!("claims={CLAIM}  not_a_proof_of={not_a_proof_of}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_is_software_fact() {
        let line = claim_line("OP1–OP6");
        assert!(line.starts_with("claims=Software fact"));
        assert!(line.contains("not_a_proof_of=OP1–OP6"));
    }
}
