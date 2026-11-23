# Feature Specification: Cosmos (Host Galaxy, Solar System & Celestial Sky)

**Feature Branch**: `001-cosmos`

**Created**: 2026-11-23

**Status**: Implemented (Baseline Spec)

**Input**: Baseline specification for the Cosmos simulation part in Tec11.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Inspect Host Galaxy, Solar System & Celestial Sphere (Priority: P1)

A user running a World simulation opens the **Cosmos** modal to view the planet's galactic vantage point, its host star and planetary system orbits, its local calendar lengths (year, synodic month, day), and its northern and southern celestial hemispheres with constellations and visible planets.

**Why this priority**: The astronomical context establishes the physical boundary conditions (insolation, spectrum, orbital period, moons, and tides) for the entire planetary simulation.

**Independent Test**: Open the World simulation, click **Cosmos**, and verify that the 3D host galaxy, interactive solar system map, calendar summary bar, and dual celestial hemisphere star charts render consistently with the planet's star, moon, and orbital parameters.

**Acceptance Scenarios**:

1. **Given** an active World simulation with a host star and 1+ moons, **When** the user clicks **Cosmos**, **Then** the modal displays the host galaxy type and galactocentric position, the solar system with the home planet marked, the calendar summary (`Year`, `Month`, `Day`), and both celestial hemispheres with constellations and visible planets.
2. **Given** a moonless World simulation (`Moons: 0`), **When** the user opens **Cosmos**, **Then** the calendar summary displays year and day lengths without a lunar synodic month, and solar-only tides are reflected.

---

### User Story 2 - Sky, Sunset & Foliage Color Emergence from Star & Atmosphere (Priority: P2)

A user selects or randomizes the host star spectral class (e.g., M-class red dwarf, K-class orange, G-class yellow, F-class white) and atmospheric composition (e.g., N2-O2 + O3, CO2-rich, H2-He Hycean, CH4 haze), and observes physically derived daytime sky color, sunset color, and photosynthetic plant foliage color once an atmosphere and biosphere develop.

**Why this priority**: Atmospheric optics connect stellar physics directly to the visual appearance of the planet and local tile sky views.

**Independent Test**: Change the host star or atmospheric gas selection before/during simulation and inspect the resulting sky, sunset, and plant foliage swatches in the World UI and Sky modal.

**Acceptance Scenarios**:

1. **Given** a planet in early Hadean Tectonics with no atmosphere yet, **When** inspecting the local sky view, **Then** the sky renders airless (black with visible stars even in daytime).
2. **Given** a planet with an active atmosphere and biosphere, **When** the star class or gas composition changes, **Then** Rayleigh/Mie scattering and pigment absorption recalculate daytime sky, sunset, and plant foliage colors accordingly.

---

### User Story 3 - Lunar Orbital Evolution & Tidal Forcing (Priority: P3)

Over geological time in Tectonics, moons migrate tidally outward toward their target orbital distance while slowing the planet's rotation toward the target day length, dynamically changing tidal strength and influencing coastal erosion and downstream tides/weather.

**Why this priority**: Couples astronomical configuration to geological and oceanographic dynamics rather than treating moons as purely decorative.

**Independent Test**: Create a planet with a close, massive moon, step through Tectonics, and observe day length lengthening, moon distance increasing toward its target, and tidal strength decreasing over hundreds of millions of years.

**Acceptance Scenarios**:

1. **Given** a newly created planet with 1 or 2 moons, **When** Tectonics steps forward, **Then** the current day length approaches the target day length and lunar orbital distance evolves smoothly toward its target distance.
2. **Given** a World snapshot saved and reloaded (or copied into the Weather/Tides sandbox), **When** reconstructed, **Then** the exact host star, solar system, galaxy vantage, moon parameters, and constellation seed are preserved.

---

### Edge Cases

- What happens when `Moons: 0` is selected? Synodic month is omitted from the Cosmos calendar display, lunar calendar vocabulary is omitted in History, and tidal forcing falls back to solar tides alone.
- What happens when a Sol-like preset is used vs. a randomized procedural star system? Sol-like systems use the Hipparcos star catalog while procedural systems deterministically synthesize star fields and constellations from the planet's constellation seed and galactic vantage.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST generate a deterministic host galaxy (type and galactocentric vantage), host star (spectral class, temperature, RGB color), planetary system (orbiting planets, rings, home planet orbit), and moon configuration from the planet's seed and parameters.
- **FR-002**: System MUST compute local orbital year length (in local days), lunar synodic month length (when at least one moon exists), and solar day length, and display them in the Cosmos modal.
- **FR-003**: System MUST render northern and southern celestial hemisphere star charts showing the galactic plane, ecliptic, visible planets, and constellations.
- **FR-004**: System MUST derive daytime sky color, sunset color, and photosynthetic foliage color from the host star spectrum and atmospheric gas composition.
- **FR-005**: System MUST persist all astronomical, stellar, atmospheric, and constellation parameters inside `PlanetSnapshot` so reloading or sandboxing reproduces the exact cosmos.

### Key Entities

- **HostStar**: Spectral class, effective temperature (K), luminosity, and RGB emission color.
- **SolarSystem**: Ordered list of planets (type, radius, semi-major axis, orbital period, rings, apparent magnitude) with one designated home planet.
- **MoonSystem**: Moon count (0, 1, or 2 harmonic), mass (in lunar masses), current and target orbital distance (in planetary radii), and resulting tidal strength.
- **AtmosphereOptics**: Atmospheric gas preset, Rayleigh/haze scattering parameters, daytime sky RGB, sunset RGB, and photosynthetic foliage RGB.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Opening the Cosmos modal renders all three canvases (3D galaxy, solar system, and dual celestial hemispheres) interactively within 1 second in a desktop browser.
- **SC-002**: Saving and reloading a `PlanetSnapshot` produces identical host star, solar system planet count, orbital lengths, and sky optics.
- **SC-003**: Moonless worlds cleanly omit synodic month calculations with zero NaN/infinity values in UI readouts or downstream weather/history models.

## Assumptions

- Host star luminosity is held constant at its main-sequence value across the simulated Tectonics–History span.
- Multi-moon configurations use a stable 2:1 resonant harmonic pair.
