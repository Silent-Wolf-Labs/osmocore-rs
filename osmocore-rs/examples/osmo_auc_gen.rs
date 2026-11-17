// SPDX-License-Identifier: GPL-3.0-or-later

//! Prints a GSM or UMTS authentication vector for given keys, or resynchronizes from a
//! USIM's AUTS first, as libosmocore's `osmo-auc-gen` utility does.
//!
//! Run with `cargo run --features auth --example osmo_auc_gen -- -3 -a milenage -k K -o OPC`.
//! `--help` lists the options. Without `-r`, RAND comes from `/dev/urandom`.
//!
//! It takes the same options and gives the same output, exit status and messages as the C
//! program, apart from its banner and help text, so it can stand in for it in scripts. The
//! osmocore-rs tests run it with every command of the C program's test script and more,
//! and compare the results with recordings of the C program. Option errors are reported as
//! glibc's `getopt_long()` reports them.

use osmocore_rs::base::{base64, hexparse};
use osmocore_rs::gsm::auth::{
    AUTS_BYTES, Algorithm, GsmAlgorithm, GsmVector, K, Ki, Op, Opc, OperatorKey, RAND_BYTES,
    ResLength, UmtsAlgorithm, UmtsSubscriber, UmtsVector,
};
use std::io::{self, Read, Write};

/// The banner: everything before the first empty line of stdout.
pub const BANNER: &str = "osmo_auc_gen: the osmocore-rs authentication vector example\n\n";

/// Printed on stdout for `--help` and before an option error. The algorithms follow on
/// stderr.
pub const HELP: &str = "\
Options:
  -2, --2g             GSM authentication
  -3, --3g             UMTS authentication
  -a, --algorithm NAME the algorithm (listed below)
  -k, --key HEX        Ki (GSM) or K (UMTS)
  -o, --opc HEX        OPc (UMTS)
  -O, --op HEX         OP, from which OPc is derived (UMTS)
  -f, --amf HEX        AMF (UMTS)
  -s, --sqn NUMBER     the SQN to use (UMTS)
  -i, --ind NUMBER     this client's IND slot in SQN (UMTS)
  -l, --ind-len BITS   the IND length, 5 by default (UMTS)
  -L, --res-len BYTES  the RES length: 4, 8 (the default) or 16 (UMTS)
  -A, --auts HEX       resynchronize from this AUTS first (UMTS)
  -r, --rand HEX       RAND, random by default
  -I                   print a strongswan triplets.dat line (GSM)
  -h, --help           print this help
";

/// The program name in messages.
const PROGRAM: &str = "osmo-auc-gen";
/// The IND length `-3` sets.
const DEFAULT_IND_BITLEN: u32 = 5;

/// How a run ends early: the exit status, after the messages are written.
type Exit = u8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Type {
    Gsm,
    Umts,
}

/// One option as `getopt_long()` would return it, with its argument.
enum Parsed<'a> {
    Option(char, Option<&'a str>),
    /// The message for an option error.
    Error(String),
}

/// The options: short name, long name and whether it takes an argument. `-I` has no long
/// name, as in C.
const OPTIONS: &[(char, Option<&str>, bool)] = &[
    ('2', Some("2g"), false),
    ('3', Some("3g"), false),
    ('a', Some("algorithm"), true),
    ('k', Some("key"), true),
    ('o', Some("opc"), true),
    ('O', Some("op"), true),
    ('f', Some("amf"), true),
    ('s', Some("sqn"), true),
    ('i', Some("ind"), true),
    ('l', Some("ind-len"), true),
    ('L', Some("res-len"), true),
    ('r', Some("rand"), true),
    ('A', Some("auts"), true),
    ('h', Some("help"), false),
    ('I', None, false),
];

/// Splits the arguments into options and positional arguments the way GNU `getopt_long()`
/// does: options may follow positional arguments, `--` ends the options, short options
/// can be grouped, and a long option can be abbreviated to any prefix that names only one.
struct Options<'a> {
    args: &'a [String],
    next: usize,
    /// The rest of a group of short options.
    group: &'a str,
    positional: Vec<&'a str>,
}

impl<'a> Options<'a> {
    fn new(args: &'a [String]) -> Self {
        Self {
            args,
            next: 0,
            group: "",
            positional: Vec::new(),
        }
    }

    fn take_next(&mut self) -> Option<&'a str> {
        let arg = self.args.get(self.next)?;
        self.next += 1;
        Some(arg)
    }

    fn short(&mut self) -> Parsed<'a> {
        let mut chars = self.group.chars();
        let c = chars.next().expect("a short option");
        self.group = chars.as_str();
        let Some(&(_, _, takes_arg)) = OPTIONS.iter().find(|(name, _, _)| *name == c) else {
            return Parsed::Error(format!("{PROGRAM}: invalid option -- '{c}'"));
        };
        if !takes_arg {
            return Parsed::Option(c, None);
        }
        if !self.group.is_empty() {
            let arg = std::mem::take(&mut self.group);
            return Parsed::Option(c, Some(arg));
        }
        match self.take_next() {
            Some(arg) => Parsed::Option(c, Some(arg)),
            None => Parsed::Error(format!("{PROGRAM}: option requires an argument -- '{c}'")),
        }
    }

    fn long(&mut self, text: &'a str) -> Parsed<'a> {
        let (name, value) = match text.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (text, None),
        };
        let named = |(short, long, takes_arg): &(char, Option<&'static str>, bool)| {
            long.map(|long| (*short, long, *takes_arg))
        };
        let exact = OPTIONS
            .iter()
            .filter_map(named)
            .find(|(_, long, _)| *long == name);
        let found = match exact {
            Some(found) => found,
            None => {
                let matches: Vec<_> = OPTIONS
                    .iter()
                    .filter_map(named)
                    .filter(|(_, long, _)| long.starts_with(name))
                    .collect();
                match matches.as_slice() {
                    [] => {
                        return Parsed::Error(format!("{PROGRAM}: unrecognized option '--{text}'"));
                    }
                    [found] => *found,
                    several => {
                        let names: String = several
                            .iter()
                            .map(|(_, long, _)| format!(" '--{long}'"))
                            .collect();
                        return Parsed::Error(format!(
                            "{PROGRAM}: option '--{text}' is ambiguous; possibilities:{names}"
                        ));
                    }
                }
            }
        };
        let (short, long, takes_arg) = found;
        match (takes_arg, value) {
            (false, None) => Parsed::Option(short, None),
            (false, Some(_)) => Parsed::Error(format!(
                "{PROGRAM}: option '--{long}' doesn't allow an argument"
            )),
            (true, Some(value)) => Parsed::Option(short, Some(value)),
            (true, None) => match self.take_next() {
                Some(arg) => Parsed::Option(short, Some(arg)),
                None => Parsed::Error(format!("{PROGRAM}: option '--{long}' requires an argument")),
            },
        }
    }
}

impl<'a> Iterator for Options<'a> {
    type Item = Parsed<'a>;

    fn next(&mut self) -> Option<Parsed<'a>> {
        if !self.group.is_empty() {
            return Some(self.short());
        }
        loop {
            let arg = self.take_next()?;
            if arg == "--" {
                self.positional
                    .extend(self.args[self.next..].iter().map(String::as_str));
                self.next = self.args.len();
                return None;
            }
            if let Some(long) = arg.strip_prefix("--") {
                return Some(self.long(long));
            }
            match arg.strip_prefix('-') {
                Some(group) if !group.is_empty() => {
                    self.group = group;
                    return Some(self.short());
                }
                _ => self.positional.push(arg),
            }
        }
    }
}

/// Parses hex into `dst` as libosmocore's `osmo_hexparse()` does, returning the byte count
/// or -1, which the messages print.
fn hex(text: &str, dst: &mut [u8]) -> i64 {
    hexparse::parse_into(text.as_bytes(), dst).map_or(-1, |len| len as i64)
}

/// Splits off C's leading whitespace and sign: whether it is negative, and the rest.
fn c_sign(text: &str) -> (bool, &str) {
    let text = text.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

/// `strtoull(text, NULL, 0)`: a `0x` prefix gives hex and a leading `0` octal. Text that
/// isn't a number gives 0, an overflow gives the largest value even when negative, and a
/// minus sign negates the value modulo 2^64.
fn c_strtoull(text: &str) -> u64 {
    let (negative, text) = c_sign(text);
    let hex_digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_hexdigit()));
    let (radix, digits) = match hex_digits {
        Some(rest) => (16, rest),
        None if text.starts_with('0') => (8, text),
        None => (10, text),
    };
    let value = digits
        .chars()
        .map_while(|c| c.to_digit(radix))
        .try_fold(0u64, |value, digit| {
            value
                .checked_mul(u64::from(radix))?
                .checked_add(u64::from(digit))
        });
    match value {
        None => u64::MAX,
        Some(value) if negative => value.wrapping_neg(),
        Some(value) => value,
    }
}

/// `atoi()` as glibc gives it: `strtol()` in base 10, saturating at 64 bits, then
/// truncated to `int`.
fn c_atoi(text: &str) -> i32 {
    let (negative, text) = c_sign(text);
    let value = text
        .chars()
        .map_while(|c| c.to_digit(10))
        .fold(0i64, |value, digit| {
            let value = value.saturating_mul(10);
            if negative {
                value.saturating_sub(digit.into())
            } else {
                value.saturating_add(digit.into())
            }
        });
    value as i32
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Everything the options set.
struct Settings {
    kind: Option<Type>,
    algorithm: Option<Algorithm>,
    ki: [u8; 16],
    k: Option<K>,
    operator: Option<OperatorKey>,
    amf: [u8; 2],
    sqn: Option<u64>,
    ind: Option<u32>,
    ind_bitlen: u32,
    res_length: ResLength,
    rand: Option<[u8; RAND_BYTES]>,
    auts: Option<[u8; AUTS_BYTES]>,
    triplets: bool,
}

/// The output on the way to the writers.
struct Output<'w, O, E> {
    out: &'w mut O,
    err: &'w mut E,
}

impl<O: Write, E: Write> Output<'_, O, E> {
    fn out(&mut self, text: &str) -> io::Result<()> {
        self.out.write_all(text.as_bytes())
    }

    fn err(&mut self, text: &str) -> io::Result<()> {
        self.err.write_all(text.as_bytes())
    }

    fn help(&mut self) -> io::Result<()> {
        self.out(HELP)?;
        self.err("\nAlgorithms for -a:\n")?;
        for algorithm in (1..).map_while(Algorithm::from_code) {
            self.err(&format!("  {algorithm}\n"))?;
        }
        Ok(())
    }

    /// An error message on stderr, then the exit status.
    fn fail(&mut self, status: Exit, message: &str) -> io::Result<Exit> {
        self.err(&format!("{message}\n"))?;
        Ok(status)
    }

    /// Help, then an error message.
    fn fail_with_help(&mut self, message: &str) -> io::Result<Exit> {
        self.help()?;
        self.fail(2, &format!("\n{message}"))
    }
}

fn parse_options<O: Write, E: Write>(
    args: &[String],
    output: &mut Output<'_, O, E>,
) -> io::Result<Result<Settings, Exit>> {
    let mut settings = Settings {
        kind: None,
        algorithm: None,
        ki: [0; 16],
        k: None,
        operator: None,
        amf: [0; 2],
        sqn: None,
        ind: None,
        ind_bitlen: 0,
        res_length: ResLength::Eight,
        rand: None,
        auts: None,
        triplets: false,
    };
    let mut options = Options::new(args);
    for parsed in options.by_ref() {
        let (option, arg) = match parsed {
            Parsed::Option(option, arg) => (option, arg.unwrap_or("")),
            Parsed::Error(message) => {
                output.err(&format!("{message}\n"))?;
                output.help()?;
                return Ok(Err(1));
            }
        };
        let umts = settings.kind == Some(Type::Umts);
        let only_umts = |what: &str| format!("Only UMTS has {what}");
        let fail = |output: &mut Output<'_, O, E>, message: String| -> io::Result<_> {
            output.fail(2, &message).map(Err)
        };
        match option {
            '2' => settings.kind = Some(Type::Gsm),
            '3' => {
                settings.kind = Some(Type::Umts);
                settings.ind_bitlen = DEFAULT_IND_BITLEN;
            }
            'a' => {
                if arg.eq_ignore_ascii_case("none") {
                    settings.algorithm = None;
                } else if let Some(algorithm) = Algorithm::parse(arg) {
                    settings.algorithm = Some(algorithm);
                } else {
                    return output
                        .fail_with_help("Error parsing argument of option `a'")
                        .map(Err);
                }
            }
            'k' => match settings.kind {
                Some(Type::Gsm) => {
                    let len = hex(arg, &mut settings.ki);
                    if len != 16 {
                        return fail(output, format!("Invalid Ki length {len}"));
                    }
                }
                Some(Type::Umts) => {
                    let mut key = [0; 32];
                    let len = hex(arg, &mut key);
                    match K::new(&key[..len.max(0) as usize]) {
                        Ok(k) => settings.k = Some(k),
                        _ => return fail(output, format!("Invalid K length {len}")),
                    }
                }
                None => output.err("please specify 2g/3g first!\n")?,
            },
            'o' | 'O' => {
                let what = if option == 'o' { "OPC" } else { "OP" };
                if !umts {
                    return fail(output, only_umts(what));
                }
                let mut key = [0; 32];
                let len = hex(arg, &mut key);
                let bytes = &key[..len.max(0) as usize];
                let operator = if option == 'o' {
                    Opc::new(bytes).map(OperatorKey::Opc)
                } else {
                    Op::new(bytes).map(OperatorKey::Op)
                };
                match operator {
                    Ok(operator) => settings.operator = Some(operator),
                    _ => return fail(output, format!("Invalid {what} length {len}")),
                }
            }
            'A' => {
                if !umts {
                    return fail(output, only_umts("AUTS"));
                }
                // A short AUTS is padded with zeros, as in C.
                let mut auts = [0; AUTS_BYTES];
                if hex(arg, &mut auts) < 0 {
                    return output
                        .fail_with_help("Error parsing argument of option `A'")
                        .map(Err);
                }
                settings.auts = Some(auts);
            }
            'f' => {
                if !umts {
                    return fail(output, only_umts("AMF"));
                }
                let len = hex(arg, &mut settings.amf);
                if len != 2 {
                    return fail(output, format!("Invalid AMF length {len}"));
                }
            }
            's' => {
                if !umts {
                    return fail(output, only_umts("SQN"));
                }
                settings.sqn = Some(c_strtoull(arg));
            }
            'i' => {
                if !umts {
                    return fail(output, only_umts("IND"));
                }
                settings.ind = Some(c_atoi(arg) as u32);
            }
            'l' => {
                if !umts {
                    return fail(output, only_umts("IND bitlen"));
                }
                settings.ind_bitlen = c_atoi(arg) as u32;
            }
            'L' => {
                let len = c_atoi(arg);
                settings.res_length = match len {
                    4 => ResLength::Four,
                    8 => ResLength::Eight,
                    16 => ResLength::Sixteen,
                    _ => return fail(output, format!("Invalid RES length {}", len as u32)),
                };
            }
            'r' => {
                let mut rand = [0; RAND_BYTES];
                let len = hex(arg, &mut rand);
                if len != RAND_BYTES as i64 {
                    return fail(output, format!("Invalid RAND length {len}"));
                }
                settings.rand = Some(rand);
            }
            'I' => settings.triplets = true,
            'h' => {
                output.help()?;
                return Ok(Err(0));
            }
            _ => unreachable!("every option is handled"),
        }
    }
    if !options.positional.is_empty() {
        return output
            .fail(2, "Unsupported positional arguments in command line")
            .map(Err);
    }
    Ok(Ok(settings))
}

/// A secure random RAND.
fn random_rand() -> io::Result<[u8; RAND_BYTES]> {
    let mut rand = [0; RAND_BYTES];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut rand)?;
    Ok(rand)
}

/// The vector for a GSM subscriber. Only the GSM algorithms have one.
fn gsm_vector(settings: &Settings, rand: &[u8; RAND_BYTES]) -> Option<GsmVector> {
    let algorithm = match settings.algorithm? {
        Algorithm::Comp128v1 => GsmAlgorithm::Comp128v1,
        Algorithm::Comp128v2 => GsmAlgorithm::Comp128v2,
        Algorithm::Comp128v3 => GsmAlgorithm::Comp128v3,
        Algorithm::Xor2g => GsmAlgorithm::Xor2g,
        Algorithm::Xor3g => GsmAlgorithm::Xor3g,
        Algorithm::Milenage | Algorithm::Tuak => return None,
    };
    Some(algorithm.vector(&Ki::new(settings.ki), rand))
}

/// The UMTS algorithm, if the algorithm is one.
fn umts_algorithm(algorithm: Algorithm) -> Option<UmtsAlgorithm> {
    match algorithm {
        Algorithm::Milenage => Some(UmtsAlgorithm::Milenage),
        Algorithm::Tuak => Some(UmtsAlgorithm::Tuak),
        Algorithm::Xor3g => Some(UmtsAlgorithm::Xor3g),
        _ => None,
    }
}

/// Runs the program with the arguments after its name, and returns its exit status.
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    let mut output = Output { out, err };
    output.out(BANNER)?;
    let settings = match parse_options(args, &mut output)? {
        Ok(settings) => settings,
        Err(status) => return Ok(status),
    };
    let rand = match settings.rand {
        Some(rand) => rand,
        None => match random_rand() {
            Ok(rand) => rand,
            Err(error) => {
                let message = format!("\nError: unable to obtain secure random numbers: {error}!");
                return output.fail(3, &message);
            }
        },
    };
    let (Some(kind), Some(algorithm)) = (settings.kind, settings.algorithm) else {
        return output.fail_with_help(
            "Error: you need to pass at least -2 or -3, as well as an algorithm to use.",
        );
    };
    let failure = if settings.auts.is_some() {
        "AUTS from MS seems incorrect"
    } else {
        "error generating auth vector"
    };

    if kind == Type::Gsm {
        // C has no resync for a GSM subscriber.
        let vector = match (settings.auts, gsm_vector(&settings, &rand)) {
            (None, Some(vector)) => vector,
            _ => return output.fail(1, failure),
        };
        let (sres, kc) = (hex_string(&vector.sres), hex_string(vector.kc.as_bytes()));
        let rand = hex_string(&rand);
        if settings.triplets {
            output.out(&format!("imsi,{rand},{sres},{kc}\n"))?;
        } else {
            output.out(&format!("RAND:\t{rand}\nSRES:\t{sres}\nKc:\t{kc}\n"))?;
        }
        return Ok(0);
    }

    // IND is the low bits of SQN. --sqn names the SQN to use, so the subscriber starts one
    // IND step before it, in its IND slot.
    let step = 1u64.checked_shl(settings.ind_bitlen).unwrap_or(0);
    let ind_mask = step.wrapping_sub(1);
    let mut sqn = 0;
    let mut ind = 0;
    if let Some(requested) = settings.sqn {
        sqn = requested.wrapping_sub(step);
        ind = (requested & ind_mask) as u32;
        if settings.ind.is_some() {
            return output.fail(
                2,
                &format!(
                    "Requesting --sqn {requested} implies IND={ind}, so no further --ind argument is allowed."
                ),
            );
        }
    }
    if let Some(requested) = settings.ind {
        if step != 0 && u64::from(requested) >= step {
            return output.fail(
                2,
                &format!(
                    "Requested --ind {requested} is too large for IND bitlen of {}",
                    settings.ind_bitlen
                ),
            );
        }
        ind = requested;
    }

    let (Some(umts), Some(k)) = (umts_algorithm(algorithm), settings.k.clone()) else {
        return output.fail(1, failure);
    };
    let operator = match (&settings.operator, umts) {
        (Some(operator), _) => operator.clone(),
        // XOR-3G doesn't use the operator key.
        (None, UmtsAlgorithm::Xor3g) => OperatorKey::Opc(Opc::from([0; 16])),
        (None, _) => return output.fail(1, failure),
    };
    let mut subscriber = UmtsSubscriber::new(umts, k, operator);
    subscriber.amf = settings.amf;
    subscriber.sqn = sqn;
    subscriber.ind_bitlen = settings.ind_bitlen;
    subscriber.ind = ind;

    let result = match &settings.auts {
        Some(auts) => subscriber
            .resync(auts, &rand, &rand, settings.res_length)
            .map(|resync| (resync.vector, Some(resync.sqn_ms))),
        None => subscriber
            .vector(&rand, settings.res_length)
            .map(|vector| (vector, None)),
    };
    let Ok((vector, sqn_ms)) = result else {
        return output.fail(1, failure);
    };
    if settings.triplets {
        return output.fail(0, "triplets.dat doesn't support UMTS!");
    }
    print_umts(&mut output, &rand, &vector)?;
    output.out(&format!(
        "SQN:\t{}\nIND:\t{}\n",
        subscriber.sqn,
        (subscriber.sqn & ind_mask) as u32
    ))?;
    if let Some(sqn_ms) = sqn_ms {
        output.out(&format!("SQN.MS:\t{sqn_ms}\n"))?;
    }
    Ok(0)
}

fn print_umts<O: Write, E: Write>(
    output: &mut Output<'_, O, E>,
    rand: &[u8; RAND_BYTES],
    vector: &UmtsVector,
) -> io::Result<()> {
    let base64 = |bytes: &[u8]| base64::encode(bytes).expect("a vector fits");
    let nonce = [&rand[..], &vector.autn].concat();
    let res = vector.res.as_bytes();
    output.out(&format!(
        "RAND:\t{}\nAUTN:\t{}\nIK:\t{}\nCK:\t{}\nRES:\t{}\nIMS nonce:\t{}\nIMS res:\t{}\n",
        hex_string(rand),
        hex_string(&vector.autn),
        hex_string(vector.ik.as_bytes()),
        hex_string(vector.ck.as_bytes()),
        hex_string(res),
        base64(&nonce),
        base64(res),
    ))?;
    output.out(&format!(
        "SRES:\t{}\nKc:\t{}\n",
        hex_string(&vector.sres),
        hex_string(vector.kc.as_bytes())
    ))
}

// The tests include this file and call `run`.
#[cfg_attr(test, allow(dead_code))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let status =
        run(&args, &mut io::stdout().lock(), &mut io::stderr().lock()).unwrap_or_else(|error| {
            eprintln!("{PROGRAM}: {error}");
            1
        });
    std::process::exit(status.into());
}
