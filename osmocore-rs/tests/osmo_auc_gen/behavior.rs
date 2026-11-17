//! What the recordings can't show: a random RAND, and the cases the harness leaves out
//! because C's result there comes from its key union or an undefined shift.

use crate::reference_vectors::run_example;

const K: &str = "EB215756028D60E3275E613320AEC880";
const OPC: &str = "FB2A3D1B360F599ABAB99DB8669F8308";
const RAND: &str = "39fa2f4e3d523d8619a73b4f65c3e14d";
const AUTS: &str = "979498b1f72d3e28c59fa2e72f9c";

fn run(args: &[&str]) -> (u8, String, String) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run_example(&args)
}

fn rand_line(stdout: &str) -> &str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("RAND:\t"))
        .expect("a RAND line")
}

#[test]
fn without_a_rand_each_run_draws_one() {
    let args = ["-3", "-a", "milenage", "-k", K, "-o", OPC];
    let (first, second) = (run(&args), run(&args));
    for (status, stdout, stderr) in [&first, &second] {
        assert_eq!((*status, stderr.as_str()), (0, ""));
        let rand = rand_line(stdout);
        assert_eq!(rand.len(), 32);
        assert!(rand.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    assert_ne!(rand_line(&first.1), rand_line(&second.1));
}

#[test]
fn a_gsm_algorithm_has_no_umts_vector() {
    // C reads the Ki of COMP128 from the first 16 bytes of the OPc buffer here.
    for algorithm in ["comp128v1", "comp128v2", "comp128v3"] {
        let (status, _, stderr) = run(&["-3", "-a", algorithm, "-k", K, "-o", OPC, "-r", RAND]);
        assert_eq!(
            (status, stderr.as_str()),
            (1, "error generating auth vector\n")
        );
    }
}

#[test]
fn a_gsm_subscriber_has_no_resync() {
    // C resynchronizes a GSM subscriber with XOR-3G, given -A before -2.
    let (status, stdout, stderr) =
        run(&["-3", "-A", AUTS, "-2", "-a", "xor-3g", "-k", K, "-r", RAND]);
    assert_eq!(
        (status, stderr.as_str()),
        (1, "AUTS from MS seems incorrect\n")
    );
    assert!(!stdout.contains("RAND:"));
}

#[test]
fn keys_stay_with_their_subscriber_type() {
    // In C, an OPc given under -3 becomes the Ki after a later -2. Here the Ki stays zero,
    // so XOR-2G gives RAND itself.
    let (status, stdout, _) = run(&["-3", "-o", OPC, "-2", "-a", "xor-2g", "-r", RAND]);
    assert_eq!(status, 0);
    assert!(stdout.contains(&format!("SRES:\t{}\n", &RAND[..8])));
    // A K given under -2 isn't a UMTS key after a later -3, so there is no vector.
    let (status, _, stderr) = run(&["-2", "-k", K, "-3", "-a", "milenage", "-o", OPC, "-r", RAND]);
    assert_eq!(
        (status, stderr.as_str()),
        (1, "error generating auth vector\n")
    );
}

#[test]
fn ind_is_checked_against_any_ind_length() {
    // The C check shifts an int, which is undefined from 31 bits.
    for bits in ["31", "40"] {
        let (status, _, stderr) = run(&[
            "-3", "-a", "milenage", "-k", K, "-o", OPC, "-r", RAND, "-l", bits, "--ind", "1",
        ]);
        assert_eq!(
            (status, stderr.as_str()),
            (1, "error generating auth vector\n")
        );
    }
    // From 64 bits, C's shift for the SQN step is undefined too.
    let (status, _, stderr) = run(&[
        "-3", "-a", "milenage", "-k", K, "-o", OPC, "-r", RAND, "-l", "64", "-s", "5",
    ]);
    assert_eq!(
        (status, stderr.as_str()),
        (1, "error generating auth vector\n")
    );
}
