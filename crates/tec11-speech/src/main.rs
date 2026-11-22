//! Native CLI driver for `tec11_speech`.
//!
//! Plays synthesized PCM via `paplay` (or `pw-play` / `aplay`), or writes a WAV
//! file via `--out`.

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use tec11_speech::{synthesize_ipa, to_wav_bytes, AudioBuffer, SynthOptions};

const DEMO_WORDS: &[(&str, &str)] = &[
    ("Basic English words", "həˈloʊ sʌn kaʊ naɪt"),
    ("Simple stressed word", "ˈta.ka.na"),
    ("Nasalized & front rounded vowels", "ˈmy.sõː.nã.ĩ"),
    ("Uvulars, lateral fricative & glottal", "ˈqa.ɬu.ʔi.χa"),
    ("Retroflexes & trill", "ˈʈa.ɳa.ʂu.ra"),
    ("Aspirates vs ejectives", "pʰa.tʰa.kʰa pʼa.tʼa.kʼa"),
    ("Implosives & clicks", "ɓa.ɗo.ɠu ǀa.ǃo.ǁu.ǂi"),
    ("Lexical tones (H, M, L, R, F)", "ma˥ ma˧ ma˩ ma˩˥ ma˥˩"),
];

fn play_native(audio: &AudioBuffer) -> Result<(), String> {
    let wav = to_wav_bytes(audio);
    for player in &["paplay", "pw-play", "aplay"] {
        let spawn = Command::new(player)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawn {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(&wav)
                    .map_err(|e| format!("Failed writing WAV to {player}: {e}"))?;
            }
            let status = child
                .wait()
                .map_err(|e| format!("Failed waiting for {player}: {e}"))?;
            if status.success() {
                return Ok(());
            }
            return Err(format!("{player} exited with status {status}"));
        }
    }
    Err("No native audio player found (tried paplay, pw-play, aplay); use --out".to_string())
}

fn print_usage() {
    eprintln!(
        "Usage: tec11_speech_cli [OPTIONS] <IPA>\n\
         Options:\n  \
           --pitch <HZ>    Base pitch in Hz (default: 125)\n  \
           --rate <MULT>   Speech rate multiplier (default: 1.0)\n  \
           --out <PATH>    Write 16-bit mono WAV to PATH\n  \
           --no-play       Do not play audio\n  \
           --demo          Play a showcase of phoneme classes"
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut opts = SynthOptions::default();
    let mut out_path: Option<String> = None;
    let mut no_play = false;
    let mut demo = false;
    let mut ipa: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "--pitch" => {
                i += 1;
                opts.base_f0 = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("Invalid --pitch value");
                        std::process::exit(2);
                    });
            }
            "--rate" => {
                i += 1;
                opts.rate = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("Invalid --rate value");
                        std::process::exit(2);
                    });
            }
            "--out" => {
                i += 1;
                out_path = args.get(i).cloned();
                if out_path.is_none() {
                    eprintln!("Missing --out path");
                    std::process::exit(2);
                }
            }
            "--no-play" => no_play = true,
            "--demo" => demo = true,
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {other}");
                    print_usage();
                    std::process::exit(2);
                }
                ipa = Some(other.to_string());
            }
        }
        i += 1;
    }

    if demo {
        for &(label, text) in DEMO_WORDS {
            println!("Playing [{label}]: /{text}/");
            let audio = synthesize_ipa(text, opts).unwrap_or_else(|e| {
                eprintln!("Synthesis error: {e}");
                std::process::exit(1);
            });
            if !no_play {
                if let Err(e) = play_native(&audio) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }

    let ipa = match ipa {
        Some(s) => s,
        None => {
            print_usage();
            std::process::exit(2);
        }
    };

    let audio = synthesize_ipa(&ipa, opts).unwrap_or_else(|e| {
        eprintln!("Synthesis error: {e}");
        std::process::exit(1);
    });

    if let Some(path) = out_path {
        let wav = to_wav_bytes(&audio);
        if let Err(e) = fs::write(&path, wav) {
            eprintln!("Failed writing {path}: {e}");
            std::process::exit(1);
        }
    }

    if !no_play {
        if let Err(e) = play_native(&audio) {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
