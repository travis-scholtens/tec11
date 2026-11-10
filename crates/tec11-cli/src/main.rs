use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tec11_geology::world::{
    format_with_commas, WorldCreateParams, WorldSimulation, WorldStageState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetStage {
    Tectonics,
    Prehistory,
    History,
}

impl TargetStage {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "0" | "tec" | "tectonics" => Ok(Self::Tectonics),
            "1" | "pre" | "prehistory" => Ok(Self::Prehistory),
            "2" | "hist" | "history" | "full" | "world" => Ok(Self::History),
            other => Err(format!(
                "Invalid --target-stage '{other}' (expected tectonics, prehistory, or history)"
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CliConfig {
    pub output: PathBuf,
    pub input: Option<PathBuf>,
    pub seed: Option<u64>,
    pub randomize: bool,
    pub grid_size: Option<u32>,
    pub day_hours: Option<u32>,
    pub tilt_deg: Option<u32>,
    pub land_pct: Option<u32>,
    pub atm_dt_myr: Option<u32>,
    pub life_dt_myr: Option<u32>,
    pub people_dt_myr: Option<u32>,
    pub moon_count: Option<u32>,
    pub moon_mass: Option<f32>,
    pub moon_dist_radii: Option<u32>,
    pub meteor_freq: Option<f32>,
    pub target_stage: TargetStage,
    pub prehistory_ticks: u32,
    pub history_ticks: u32,
    pub exact_steps: Option<u32>,
    pub quiet: bool,
    pub show_help: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            output: PathBuf::from("world.tec11"),
            input: None,
            seed: None,
            randomize: false,
            grid_size: None,
            day_hours: None,
            tilt_deg: None,
            land_pct: None,
            atm_dt_myr: None,
            life_dt_myr: None,
            people_dt_myr: None,
            moon_count: None,
            moon_mass: None,
            moon_dist_radii: None,
            meteor_freq: None,
            target_stage: TargetStage::History,
            prehistory_ticks: 32,
            history_ticks: 10,
            exact_steps: None,
            quiet: false,
            show_help: false,
        }
    }
}

pub fn print_usage() {
    println!(
        "\
tec11-cli — Native Rust World Simulation & Snapshot Generator

USAGE:
    bazel run //:tec11_cli -- [OPTIONS]

OPTIONS:
    -o, --output <PATH>          Output serialized .tec11 snapshot file [default: world.tec11]
    -i, --input <PATH>           Load an existing .tec11 snapshot file and continue stepping
    -s, --seed <U64>             RNG seed (defaults to system clock entropy)
    -r, --randomize              Randomize planet parameters (day length, axial tilt, land %,
                                 moons, and atmosphere/life/people evolutionary intervals) just like
                                 the GUI's 'Randomize: On' button. Any explicit parameter flag
                                 passed alongside --randomize overrides that specific parameter.

PLANET CREATION PARAMETERS (matching the World Simulation GUI):
    -g, --grid-size <4|5|6|7>    Planet size / subdivision level [default: 5]:
                                   4 = Very Small (2,100 km —    812 coarse tiles)
                                   5 = Small      (3,700 km —  2,432 coarse tiles)
                                   6 = Medium     (6,400 km —  7,292 coarse tiles, Earth)
                                   7 = Large      (11,000 km — 21,872 coarse tiles)
        --day-hours <HOURS>      Target rotation period in hours/day at Prehistory start [default: 24]
        --tilt <DEG>             Base axial tilt in degrees (0..=45) [default: 23]
        --land-pct <PCT>         Initial continental land percentage (1..=65) [default: 29]
        --atm-dt <MYR>           Time until atmosphere formation in Myr (5..=500) [default: 125]
        --life-dt <MYR>          Time from atmosphere to biosphere in Myr (5..=500) [default: 125]
        --people-dt <MYR>        Time from biosphere to people in Myr (5..=500) [default: 125]
        --moons <0..=4>          Number of moons in a 2:1 Laplace harmonic resonance chain [default: 1]
        --moon-mass <MASS>       Primary moon mass in Earth Lunar masses (0.1..=3.0) [default: 1.0]
        --moon-dist <RADII>      Target innermost moon distance at Prehistory start in R_p [default: 60]
        --meteor-freq <MULT>     Meteor / bolide impact frequency multiplier (0.0..=5.0) [default: 1.0]

SIMULATION STEPPING OPTIONS:
        --target-stage <STAGE>   Stage to run through: tectonics | prehistory | history [default: history]
        --prehistory-ticks <N>   4,000-year ticks to run in Prehistory [default: 32]
        --history-ticks <N>      200-year ticks to run in History [default: 10]
        --steps <N>              Run an exact number of world.step() calls instead of stage targets
    -q, --quiet                  Suppress progress output
    -h, --help                   Print this help message"
    );
}

fn parse_u32_flag(flag: &str, val: &str, min: u32, max: u32) -> Result<u32, String> {
    let parsed: u32 = val
        .parse()
        .map_err(|_| format!("Invalid integer value '{val}' for {flag}"))?;
    if parsed < min || parsed > max {
        return Err(format!(
            "Value {parsed} for {flag} is out of range [{min}..={max}]"
        ));
    }
    Ok(parsed)
}

fn parse_f32_flag(flag: &str, val: &str, min: f32, max: f32) -> Result<f32, String> {
    let parsed: f32 = val
        .parse()
        .map_err(|_| format!("Invalid float value '{val}' for {flag}"))?;
    if !parsed.is_finite() || parsed < min || parsed > max {
        return Err(format!(
            "Value {parsed} for {flag} is out of range [{min}..={max}]"
        ));
    }
    Ok(parsed)
}

pub fn parse_args<I, S>(args: I) -> Result<CliConfig, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cfg = CliConfig::default();
    let mut raw: Vec<String> = Vec::new();
    for a in args {
        let s = a.as_ref();
        if let Some((k, v)) = s.split_once('=') {
            if k.starts_with('-') {
                raw.push(k.to_string());
                raw.push(v.to_string());
                continue;
            }
        }
        raw.push(s.to_string());
    }

    let mut idx = 0;
    while idx < raw.len() {
        let arg = raw[idx].as_str();
        let mut next_val = |flag: &str| -> Result<String, String> {
            idx += 1;
            raw.get(idx)
                .cloned()
                .ok_or_else(|| format!("Missing value for {flag}"))
        };

        match arg {
            "-h" | "--help" => {
                cfg.show_help = true;
            }
            "-q" | "--quiet" => {
                cfg.quiet = true;
            }
            "-r" | "--randomize" | "--random" => {
                cfg.randomize = true;
            }
            "-o" | "--output" => {
                let v = next_val(arg)?;
                cfg.output = PathBuf::from(v);
            }
            "-i" | "--input" => {
                let v = next_val(arg)?;
                cfg.input = Some(PathBuf::from(v));
            }
            "-s" | "--seed" => {
                let v = next_val(arg)?;
                let parsed: u64 = v
                    .parse()
                    .map_err(|_| format!("Invalid u64 seed '{v}' for {arg}"))?;
                cfg.seed = Some(parsed);
            }
            "-g" | "--grid-size" | "--size" => {
                let v = next_val(arg)?;
                cfg.grid_size = Some(parse_u32_flag(arg, &v, 2, 7)?);
            }
            "--day-hours" | "--hours" => {
                let v = next_val(arg)?;
                cfg.day_hours = Some(parse_u32_flag(arg, &v, 1, 240)?);
            }
            "--tilt" | "--tilt-deg" => {
                let v = next_val(arg)?;
                cfg.tilt_deg = Some(parse_u32_flag(arg, &v, 0, 90)?);
            }
            "--land-pct" | "--land" => {
                let v = next_val(arg)?;
                cfg.land_pct = Some(parse_u32_flag(arg, &v, 1, 95)?);
            }
            "--atm-dt" => {
                let v = next_val(arg)?;
                cfg.atm_dt_myr = Some(parse_u32_flag(arg, &v, 5, 2000)?);
            }
            "--life-dt" => {
                let v = next_val(arg)?;
                cfg.life_dt_myr = Some(parse_u32_flag(arg, &v, 5, 2000)?);
            }
            "--people-dt" => {
                let v = next_val(arg)?;
                cfg.people_dt_myr = Some(parse_u32_flag(arg, &v, 5, 2000)?);
            }
            "--moons" | "--moon-count" => {
                let v = next_val(arg)?;
                cfg.moon_count = Some(parse_u32_flag(arg, &v, 0, 8)?);
            }
            "--moon-mass" => {
                let v = next_val(arg)?;
                cfg.moon_mass = Some(parse_f32_flag(arg, &v, 0.0, 10.0)?);
            }
            "--moon-dist" | "--moon-distance" => {
                let v = next_val(arg)?;
                cfg.moon_dist_radii = Some(parse_u32_flag(arg, &v, 15, 160)?);
            }
            "--meteor-freq" | "--meteors" | "--meteor-rate" => {
                let v = next_val(arg)?;
                cfg.meteor_freq = Some(parse_f32_flag(arg, &v, 0.0, 10.0)?);
            }
            "--target-stage" | "--stage" => {
                let v = next_val(arg)?;
                cfg.target_stage = TargetStage::parse(&v)?;
            }
            "--prehistory-ticks" => {
                let v = next_val(arg)?;
                cfg.prehistory_ticks = parse_u32_flag(arg, &v, 0, 10_000)?;
            }
            "--history-ticks" => {
                let v = next_val(arg)?;
                cfg.history_ticks = parse_u32_flag(arg, &v, 0, 10_000)?;
            }
            "--steps" => {
                let v = next_val(arg)?;
                cfg.exact_steps = Some(parse_u32_flag(arg, &v, 0, 100_000)?);
            }
            other => {
                return Err(format!("Unknown argument '{other}'. Run with --help for usage."));
            }
        }
        idx += 1;
    }

    Ok(cfg)
}

pub fn resolve_world_params(
    cfg: &CliConfig,
    seed: u64,
) -> (WorldSimulation, WorldCreateParams) {
    let mut base_params = WorldCreateParams::default();
    if let Some(g) = cfg.grid_size {
        base_params.grid_size = g;
    }
    if let Some(d) = cfg.day_hours {
        base_params.day_hours = d;
    }
    if let Some(t) = cfg.tilt_deg {
        base_params.tilt_deg = t;
    }
    if let Some(l) = cfg.land_pct {
        base_params.land_pct = l;
    }
    if let Some(a) = cfg.atm_dt_myr {
        base_params.atm_dt_myr = a;
    }
    if let Some(lf) = cfg.life_dt_myr {
        base_params.life_dt_myr = lf;
    }
    if let Some(p) = cfg.people_dt_myr {
        base_params.people_dt_myr = p;
    }
    if let Some(mc) = cfg.moon_count {
        base_params.moon_count = mc;
        if mc == 0 && cfg.moon_mass.is_none() {
            base_params.moon_mass = 0.0;
        }
    }
    if let Some(mm) = cfg.moon_mass {
        base_params.moon_mass = mm;
    }
    if let Some(md) = cfg.moon_dist_radii {
        base_params.moon_dist_radii = md;
    }
    if let Some(mf) = cfg.meteor_freq {
        base_params.meteor_freq = mf;
    }

    if !cfg.randomize {
        let world = WorldSimulation::new(base_params, seed);
        return (world, base_params);
    }

    // Generate randomized parameters using WorldSimulation::create(..., true), then apply any explicit CLI overrides
    let mut world = WorldSimulation::new(base_params, seed);
    let mut resolved = world.create(base_params, true);
    let has_override = cfg.day_hours.is_some()
        || cfg.tilt_deg.is_some()
        || cfg.land_pct.is_some()
        || cfg.atm_dt_myr.is_some()
        || cfg.life_dt_myr.is_some()
        || cfg.people_dt_myr.is_some()
        || cfg.moon_count.is_some()
        || cfg.moon_mass.is_some()
        || cfg.moon_dist_radii.is_some()
        || cfg.meteor_freq.is_some();

    if has_override {
        if let Some(d) = cfg.day_hours {
            resolved.day_hours = d;
        }
        if let Some(t) = cfg.tilt_deg {
            resolved.tilt_deg = t;
        }
        if let Some(l) = cfg.land_pct {
            resolved.land_pct = l;
        }
        if let Some(a) = cfg.atm_dt_myr {
            resolved.atm_dt_myr = a;
        }
        if let Some(lf) = cfg.life_dt_myr {
            resolved.life_dt_myr = lf;
        }
        if let Some(p) = cfg.people_dt_myr {
            resolved.people_dt_myr = p;
        }
        if let Some(mc) = cfg.moon_count {
            resolved.moon_count = mc;
            if mc == 0 && cfg.moon_mass.is_none() {
                resolved.moon_mass = 0.0;
            } else if mc > 0 && resolved.moon_mass <= 0.0 && cfg.moon_mass.is_none() {
                resolved.moon_mass = 1.0;
            }
        }
        if let Some(mm) = cfg.moon_mass {
            resolved.moon_mass = mm;
        }
        if let Some(md) = cfg.moon_dist_radii {
            resolved.moon_dist_radii = md;
        }
        if let Some(mf) = cfg.meteor_freq {
            resolved.meteor_freq = mf;
        }
        world = WorldSimulation::new(resolved, seed);
    }

    (world, resolved)
}

pub fn run_cli(cfg: &CliConfig) -> Result<Vec<u8>, String> {
    let start_time = Instant::now();
    let seed = cfg.seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42)
    });

    let mut world = if let Some(input_path) = &cfg.input {
        let bytes = fs::read(input_path).map_err(|e| {
            format!(
                "Failed to read input snapshot '{}': {e}",
                input_path.display()
            )
        })?;
        let mut w = WorldSimulation::new(WorldCreateParams::default(), seed);
        if !w.load_snapshot_bytes(&bytes) {
            return Err(format!(
                "Failed to decode PlanetSnapshot from '{}'",
                input_path.display()
            ));
        }
        if !cfg.quiet {
            println!(
                "Loaded snapshot '{}' at stage '{}' ({} years)",
                input_path.display(),
                w.stage_name(),
                w.years_formatted()
            );
        }
        w
    } else {
        let (w, params) = resolve_world_params(cfg, seed);
        if !cfg.quiet {
            println!(
                "Initialized World Simulation (seed={seed}, randomize={}):\n  \
                 grid_size={} ({} coarse tiles), target_day={}h (Hadean={:.1}h), tilt={}°, land={}%, \
                 moons={} (mass={:.1}x, target_dist={} Rp, Hadean_dist={:.1} Rp, tides={:.1}x), \
                 meteors={:.1}x, atm_dt={} Myr, life_dt={} Myr, people_dt={} Myr",
                cfg.randomize,
                params.grid_size,
                w.tile_count(),
                params.day_hours,
                w.current_day_hours(),
                params.tilt_deg,
                params.land_pct,
                params.moon_count,
                params.moon_mass,
                params.moon_dist_radii,
                w.current_moon_dist_radii(),
                w.current_tidal_strength(),
                params.meteor_freq,
                params.atm_dt_myr,
                params.life_dt_myr,
                params.people_dt_myr
            );
        }
        w
    };

    if let Some(steps) = cfg.exact_steps {
        for i in 1..=steps {
            world.step();
            if !cfg.quiet && (i % 10 == 0 || i == steps) {
                println!(
                    "  [Step {i}/{steps}] Stage: {} | Years: {} | Pop: {:.1}K",
                    world.stage_name(),
                    world.years_formatted(),
                    world.total_population_thousands()
                );
            }
        }
    } else {
        // 1. Run Tectonics stage
        while world.stage_index() == 0 {
            if cfg.target_stage == TargetStage::Tectonics {
                // In tectonics-only target mode, step until biosphere + people_dt_myr countdown reaches 1
                let total_tec_ticks = ((world.params().atm_dt_myr
                    + world.params().life_dt_myr
                    + world.params().people_dt_myr)
                    / 5)
                .max(1);
                if world.tectonics_ticks() >= total_tec_ticks {
                    break;
                }
            }
            world.step();
            if !cfg.quiet && world.stage_index() == 0 && world.tectonics_ticks() % 25 == 0 {
                println!(
                    "  [Tectonics] {} geoticks ({} Myr) — {}",
                    world.tectonics_ticks(),
                    world.tectonics_ticks() * 5,
                    world.stage_name()
                );
            }
        }

        if !cfg.quiet && world.stage_index() >= 1 {
            println!(
                "Completed Tectonics after {} geoticks ({} Myr).",
                world.tectonics_ticks(),
                world.tectonics_ticks() * 5
            );
        }

        // 2. Run Prehistory stage if target >= Prehistory
        if cfg.target_stage != TargetStage::Tectonics {
            if world.stage_index() == 0 {
                world.advance_stage();
            }
            while world.stage_index() == 1 && world.prehistory_ticks() < cfg.prehistory_ticks {
                world.step();
            }
            if cfg.target_stage == TargetStage::History && world.stage_index() == 1 {
                world.advance_stage();
            }
            if !cfg.quiet {
                println!(
                    "Completed Prehistory after {} ticks ({} years).",
                    world.prehistory_ticks(),
                    format_with_commas((world.prehistory_ticks() as u64) * 4_000)
                );
            }
        }

        // 3. Run History stage if target == History
        if cfg.target_stage == TargetStage::History {
            if world.stage_index() < 2 {
                world.advance_stage();
            }
            for _ in 0..cfg.history_ticks {
                world.step();
            }
            if !cfg.quiet {
                if let WorldStageState::History(hist) = world.stage() {
                    println!(
                        "Completed History after {} ticks ({} years): {} active states, {:.1}K total population.",
                        hist.history_ticks(),
                        format_with_commas(hist.years() as u64),
                        hist.active_state_count(),
                        hist.total_population_thousands()
                    );
                }
            }
        }
    }

    let bytes = world.encode_snapshot();
    if let Some(parent) = cfg.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create parent directory '{}': {e}",
                    parent.display()
                )
            })?;
        }
    }
    fs::write(&cfg.output, &bytes).map_err(|e| {
        format!(
            "Failed to write output snapshot '{}': {e}",
            cfg.output.display()
        )
    })?;

    if !cfg.quiet {
        println!(
            "Wrote {} bytes to '{}' in {:.2}s (Stage: {}, Total Elapsed: {} years).",
            format_with_commas(bytes.len() as u64),
            cfg.output.display(),
            start_time.elapsed().as_secs_f64(),
            world.stage_name(),
            world.years_formatted()
        );
    }

    Ok(bytes)
}

fn main() -> ExitCode {
    let cfg = match parse_args(env::args().skip(1)) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if cfg.show_help {
        print_usage();
        return ExitCode::SUCCESS;
    }

    match run_cli(&cfg) {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_explicit_and_random() {
        let cfg = parse_args([
            "--randomize",
            "--grid-size",
            "4",
            "--day-hours=12",
            "--tilt",
            "18",
            "--land-pct",
            "35",
            "--atm-dt",
            "10",
            "--life-dt",
            "15",
            "--people-dt",
            "20",
            "--moons",
            "2",
            "--moon-mass",
            "1.5",
            "--moon-dist",
            "55",
            "--meteor-freq",
            "2.5",
            "--history-ticks",
            "3",
            "-o",
            "/tmp/test_world.tec11",
        ])
        .expect("valid CLI args");

        assert!(cfg.randomize);
        assert_eq!(cfg.grid_size, Some(4));
        assert_eq!(cfg.day_hours, Some(12));
        assert_eq!(cfg.tilt_deg, Some(18));
        assert_eq!(cfg.land_pct, Some(35));
        assert_eq!(cfg.atm_dt_myr, Some(10));
        assert_eq!(cfg.life_dt_myr, Some(15));
        assert_eq!(cfg.people_dt_myr, Some(20));
        assert_eq!(cfg.moon_count, Some(2));
        assert_eq!(cfg.moon_mass, Some(1.5));
        assert_eq!(cfg.moon_dist_radii, Some(55));
        assert_eq!(cfg.meteor_freq, Some(2.5));
        assert_eq!(cfg.history_ticks, 3);
        assert_eq!(cfg.output, PathBuf::from("/tmp/test_world.tec11"));
    }

    #[test]
    fn test_cli_full_simulation_and_snapshot_roundtrip() {
        let out_path = env::temp_dir().join("tec11_cli_test_snapshot.tec11");
        let cfg = CliConfig {
            output: out_path.clone(),
            input: None,
            seed: Some(42),
            randomize: true,
            grid_size: Some(4),
            day_hours: None,
            tilt_deg: None,
            land_pct: None,
            atm_dt_myr: Some(5),
            life_dt_myr: Some(5),
            people_dt_myr: Some(5),
            moon_count: None,
            moon_mass: None,
            moon_dist_radii: None,
            meteor_freq: None,
            target_stage: TargetStage::History,
            prehistory_ticks: 4,
            history_ticks: 2,
            exact_steps: None,
            quiet: true,
            show_help: false,
        };

        let bytes = run_cli(&cfg).expect("CLI run should succeed");
        assert!(!bytes.is_empty());
        assert!(out_path.exists());

        // Verify loading the generated file back via --input and stepping 1 more tick
        let out_path_2 = env::temp_dir().join("tec11_cli_test_snapshot_stepped.tec11");
        let cfg_reload = CliConfig {
            output: out_path_2.clone(),
            input: Some(out_path.clone()),
            seed: Some(42),
            exact_steps: Some(1),
            quiet: true,
            ..CliConfig::default()
        };
        let bytes_2 = run_cli(&cfg_reload).expect("CLI reload run should succeed");
        assert!(!bytes_2.is_empty());

        let mut verify_world = WorldSimulation::new(WorldCreateParams::default(), 42);
        assert!(verify_world.load_snapshot_bytes(&bytes_2));
        assert_eq!(verify_world.stage_index(), 2);
        assert_eq!(verify_world.history_ticks(), 3);

        let _ = fs::remove_file(out_path);
        let _ = fs::remove_file(out_path_2);
    }
}
