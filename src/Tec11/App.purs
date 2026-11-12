module Tec11.App
  ( mkApp
  ) where

import Prelude

import Data.Int as Int
import Data.Maybe (Maybe(..))
import Data.Tuple.Nested ((/\))
import Effect (Effect)
import React.Basic.DOM as R
import React.Basic.DOM.Events (capture_, targetValue)
import React.Basic.Events (handler)
import React.Basic.Hooks (Component, component, useEffect, useEffectOnce, useState')
import React.Basic.Hooks as React

data Screen
  = MainMenu
  | GridScreen
  | PangeaScreen
  | SplitScreen
  | MoveScreen
  | ErosionScreen
  | ClimateScreen
  | TectonicsScreen
  | RiversScreen
  | LifeformsScreen
  | PopulationScreen
  | RacinationScreen
  | PrehistoryScreen
  | LanguageScreen
  | TerrainScreen
  | TravelScreen
  | HistoryScreen
  | WorldScreen
  | MoonScreen
  | MeteorScreen
  | SunScreen

derive instance eqScreen :: Eq Screen

type GridInfo =
  { size :: Int
  , tileCount :: Int
  }

type PangeaInfo =
  { gridSize :: Int
  , tileCount :: Int
  , actualLandPercent :: Int
  }

type SplitInfo =
  { gridSize :: Int
  , tileCount :: Int
  , iterations :: Int
  , redCount :: Int
  , blueCount :: Int
  }

type MoveInfo =
  { gridSize :: Int
  , tileCount :: Int
  , groupTileCount :: Int
  , steps :: Int
  , isRunning :: Boolean
  }

type ErosionInfo =
  { planetName :: String
  , tileCount :: Int
  , steps :: Int
  , hasSavedPlanet :: Boolean
  , selectedTileReport :: String
  }

type ClimateInfo =
  { planetName :: String
  , tileCount :: Int
  , hasSavedPlanet :: Boolean
  }

type TectonicsInfo =
  { gridSize :: Int
  , tileCount :: Int
  , geoticks :: Int
  , continentCount :: Int
  , actualLandPercent :: Int
  , hasAtmosphere :: Boolean
  , hasLife :: Boolean
  , isRunning :: Boolean
  , selectedTileReport :: String
  }

type RiversInfo =
  { planetName :: String
  , tileCount :: Int
  , riverCount :: Int
  , hasSavedPlanet :: Boolean
  }

type LifeformsSpeciesItem =
  { id :: Int
  , kind :: String
  , name :: String
  }

type LifeformsInfo =
  { planetName :: String
  , tileCount :: Int
  , settledAnimals :: Int
  , settledPlants :: Int
  , hasSavedPlanet :: Boolean
  , selectedTileIdx :: Int
  , selectedTileRegion :: String
  , selectedSpeciesId :: Int
  , tileSpecies :: Array LifeformsSpeciesItem
  }

type PopulationInfo =
  { planetName :: String
  , tileCount :: Int
  , populatedCount :: Int
  , ticks :: Int
  , isRunning :: Boolean
  , hasSavedPlanet :: Boolean
  }

type RacinationInfo =
  { tileCount :: Int
  , peoples :: Int
  , populatedCount :: Int
  , isolations :: Int
  }

type PrehistoryInfo =
  { planetName :: String
  , tileCount :: Int
  , ticks :: Int
  , kyr :: Int
  , glaciationPct :: Int
  , races :: Int
  , agRaces :: Int
  , populatedCount :: Int
  , populationText :: String
  , isRunning :: Boolean
  , hasSavedPlanet :: Boolean
  }

type LanguageWordItem =
  { idx :: Int
  , concept :: String
  , sourceOrth :: String
  , sourceIpa :: String
  , destOrth :: String
  , destIpa :: String
  , etymology :: String
  }

type LanguageInfo =
  { sourcePhonemes :: String
  , destPhonemes :: String
  , words :: Array LanguageWordItem
  }

type TerrainInfo =
  { planetName :: String
  , displayFaceCount :: Int
  , fineFaceCount :: Int
  , coarseTileCount :: Int
  , riverCount :: Int
  , hasSavedPlanet :: Boolean
  , selectedReport :: String
  }

type HistoryInfo =
  { planetName :: String
  , tileCount :: Int
  , ticks :: Int
  , years :: Int
  , activeStates :: Int
  , populationText :: String
  , isRunning :: Boolean
  , hasSavedPlanet :: Boolean
  , selectedReport :: String
  }

type TravelInfo =
  { planetName :: String
  , tileCount :: Int
  , ticks :: Int
  , years :: Int
  , activeStates :: Int
  , populationText :: String
  , activeRoutes :: Int
  , potentialRoutes :: Int
  , portCount :: Int
  , tradeRoutes :: Int
  , conflictRoutes :: Int
  , isRunning :: Boolean
  , hasSavedPlanet :: Boolean
  , selectedReport :: String
  }

type WorldInfo =
  { stageIndex :: Int
  , stageName :: String
  , tileCount :: Int
  , yearsText :: String
  , populationText :: String
  , gridSize :: Int
  , dayHours :: Int
  , tiltDeg :: Int
  , landPct :: Int
  , atmDtMyr :: Int
  , lifeDtMyr :: Int
  , peopleDtMyr :: Int
  , moonCount :: Int
  , moonMassTenths :: Int
  , moonDistRadii :: Int
  , meteorFreqTenths :: Int
  , orbitalSummaryText :: String
  , sunFilter :: Int
  , atmosphereId :: Int
  , sunSpectralType :: String
  , sunColorCss :: String
  , atmosphereName :: String
  , skyColorCss :: String
  , skyColorName :: String
  , sunsetColorCss :: String
  , sunsetColorName :: String
  , plantColorCss :: String
  , plantColorName :: String
  , planetsSummaryText :: String
  , isRunning :: Boolean
  , hasSavedPlanet :: Boolean
  , selectedReport :: String
  }

type MoonInfo =
  { planetName :: String
  , tileCount :: Int
  , steps :: Int
  , harmonicSummary :: String
  , tidalSharePctText :: String
  , meanCoastalScourText :: String
  , maxTidalRangeText :: String
  , hasSavedPlanet :: Boolean
  , selectedReport :: String
  }

type MeteorInfo =
  { planetName :: String
  , tileCount :: Int
  , impacts :: Int
  , recoverySteps :: Int
  , impactSummary :: String
  , coolingText :: String
  , biospherePctText :: String
  , hasSavedPlanet :: Boolean
  , selectedReport :: String
  }

type SunInfo =
  { galaxyTypeName :: String
  , vantageName :: String
  , starCount :: Int
  , sunSpectralType :: String
  , sunClassDesc :: String
  , sunColorCss :: String
  , sunBvText :: String
  , sunTempText :: String
  , sunRadiusText :: String
  , sunMassText :: String
  , sunLumText :: String
  , sunHzText :: String
  , atmosphereName :: String
  , atmosphereFormula :: String
  , skyColorCss :: String
  , skyHorizonCss :: String
  , skyColorName :: String
  , skyMechanism :: String
  , sunsetColorCss :: String
  , twilightColorCss :: String
  , sunsetColorName :: String
  , sunsetMechanism :: String
  , plantColorCss :: String
  , plantHighlightCss :: String
  , plantColorName :: String
  , plantPigmentDesc :: String
  , planetsSummaryText :: String
  , summaryLine :: String
  , selectedReport :: String
  }

foreign import initWasm :: Effect Unit -> Effect Unit

-- Grid mode FFI
foreign import mountGridView :: (GridInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import subdivideGrid :: (GridInfo -> Effect Unit) -> Effect Unit

-- Pangea mode FFI
foreign import mountPangeaView :: Int -> Int -> String -> (PangeaInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import regeneratePangea :: Int -> Int -> (PangeaInfo -> Effect Unit) -> Effect Unit
foreign import setPangeaRenderMode :: String -> Effect Unit

-- Split mode FFI
foreign import mountSplitView :: Int -> Int -> (SplitInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import iterateSplit :: Int -> (SplitInfo -> Effect Unit) -> Effect Unit
foreign import resetSplit :: Int -> (SplitInfo -> Effect Unit) -> Effect Unit
foreign import setSplitTime :: Int -> Effect Unit

-- Move mode FFI
foreign import mountMoveView :: Int -> Number -> Number -> Boolean -> (MoveInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepMove :: (MoveInfo -> Effect Unit) -> Effect Unit
foreign import toggleMoveRunning :: (MoveInfo -> Effect Unit) -> Effect Unit
foreign import resetMove :: Int -> Number -> Number -> (MoveInfo -> Effect Unit) -> Effect Unit
foreign import setMoveDirection :: Number -> Effect Unit
foreign import setMoveSpeed :: Number -> Effect Unit
foreign import setMoveTrail :: Boolean -> Effect Unit

-- Erosion mode FFI
foreign import mountErosionView :: Boolean -> Boolean -> (ErosionInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepErosion :: (ErosionInfo -> Effect Unit) -> Effect Unit
foreign import setErosionOverlays :: Boolean -> Boolean -> Effect Unit
foreign import loadEarthIntoErosion :: (ErosionInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoErosion :: (ErosionInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoErosion :: (ErosionInfo -> Effect Unit) -> Effect Unit

-- Climate mode FFI
foreign import mountClimateView :: Int -> Int -> String -> Int -> (ClimateInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import setClimateAttribute :: String -> Effect Unit
foreign import setClimateSeason :: Int -> Effect Unit
foreign import setClimateParams :: Int -> Int -> Effect Unit
foreign import loadEarthIntoClimate :: (ClimateInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoClimate :: (ClimateInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoClimate :: (ClimateInfo -> Effect Unit) -> Effect Unit

-- Tectonics mode FFI
foreign import mountTectonicsView :: Int -> Int -> String -> (TectonicsInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepTectonics :: (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import toggleTectonicsRunning :: (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import resetTectonics :: Int -> Int -> (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import setTectonicsAspect :: String -> Effect Unit
foreign import saveTectonicsPlanet :: (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import loadTectonicsPlanetFromFile :: (Int -> Effect Unit) -> (TectonicsInfo -> Effect Unit) -> Effect Unit

-- Rivers mode FFI
foreign import mountRiversView :: Int -> Int -> (RiversInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import setRiversParams :: Int -> Int -> (RiversInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoRivers :: (RiversInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoRivers :: (RiversInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoRivers :: (RiversInfo -> Effect Unit) -> Effect Unit

-- Lifeforms mode FFI
foreign import mountLifeformsView :: Int -> String -> Int -> (LifeformsInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import setLifeformsAttribute :: String -> (LifeformsInfo -> Effect Unit) -> Effect Unit
foreign import setLifeformsSeason :: Int -> (LifeformsInfo -> Effect Unit) -> Effect Unit
foreign import setLifeformsGlaciation :: Int -> (LifeformsInfo -> Effect Unit) -> Effect Unit
foreign import selectLifeformsSpecies :: Int -> (LifeformsInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoLifeforms :: (LifeformsInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoLifeforms :: (LifeformsInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoLifeforms :: (LifeformsInfo -> Effect Unit) -> Effect Unit

-- Population mode FFI
foreign import mountPopulationView :: Int -> Int -> (PopulationInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepPopulation :: (PopulationInfo -> Effect Unit) -> Effect Unit
foreign import togglePopulationRunning :: (PopulationInfo -> Effect Unit) -> Effect Unit
foreign import resetPopulation :: (PopulationInfo -> Effect Unit) -> Effect Unit
foreign import setPopulationCoastProx :: Int -> Effect Unit
foreign import setPopulationTravelRange :: Int -> Effect Unit
foreign import loadEarthIntoPopulation :: (PopulationInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoPopulation :: (PopulationInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoPopulation :: (PopulationInfo -> Effect Unit) -> Effect Unit

-- Racination mode FFI
foreign import mountRacinationView :: Int -> (RacinationInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import isolateRacination :: (RacinationInfo -> Effect Unit) -> Effect Unit
foreign import resetRacination :: (RacinationInfo -> Effect Unit) -> Effect Unit
foreign import setRacinationTravelRange :: Int -> Effect Unit

-- Prehistory mode FFI
foreign import mountPrehistoryView :: (PrehistoryInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepPrehistory :: (PrehistoryInfo -> Effect Unit) -> Effect Unit
foreign import togglePrehistoryRunning :: (PrehistoryInfo -> Effect Unit) -> Effect Unit
foreign import resetPrehistory :: (PrehistoryInfo -> Effect Unit) -> Effect Unit
foreign import savePrehistoryPlanet :: (PrehistoryInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoPrehistory :: (PrehistoryInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoPrehistory :: (PrehistoryInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoPrehistory :: (PrehistoryInfo -> Effect Unit) -> Effect Unit

-- Language mode FFI
foreign import mountLanguageView :: (LanguageInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import generateLanguage :: (LanguageInfo -> Effect Unit) -> Effect Unit
foreign import borrowLanguage :: (LanguageInfo -> Effect Unit) -> Effect Unit
foreign import applyLanguageSoundChange :: Int -> (LanguageInfo -> Effect Unit) -> Effect Unit

-- Terrain mode FFI (`terrainpresenter.py`)
foreign import mountTerrainView :: Boolean -> (TerrainInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import setTerrainShowRivers :: Boolean -> Effect Unit
foreign import saveTerrainPlanet :: (TerrainInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoTerrain :: (TerrainInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoTerrain :: (TerrainInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoTerrain :: (TerrainInfo -> Effect Unit) -> Effect Unit

-- History mode FFI
foreign import mountHistoryView :: String -> Boolean -> (HistoryInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepHistory :: (HistoryInfo -> Effect Unit) -> Effect Unit
foreign import toggleHistoryRunning :: (HistoryInfo -> Effect Unit) -> Effect Unit
foreign import resetHistory :: (HistoryInfo -> Effect Unit) -> Effect Unit
foreign import setHistoryAspect :: String -> Effect Unit
foreign import setHistoryShowRivers :: Boolean -> Effect Unit
foreign import saveHistoryPlanet :: (HistoryInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoHistory :: (HistoryInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoHistory :: (HistoryInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoHistory :: (HistoryInfo -> Effect Unit) -> Effect Unit

-- Travel Routes & Maritime Navigation mode FFI (`TravelScreen`)
foreign import mountTravelView :: String -> Boolean -> Boolean -> Boolean -> (TravelInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepTravel :: (TravelInfo -> Effect Unit) -> Effect Unit
foreign import toggleTravelRunning :: (TravelInfo -> Effect Unit) -> Effect Unit
foreign import resetTravel :: (TravelInfo -> Effect Unit) -> Effect Unit
foreign import setTravelDisplayOptions :: String -> Boolean -> Boolean -> Boolean -> Effect Unit
foreign import saveTravelPlanet :: (TravelInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoTravel :: (TravelInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoTravel :: (TravelInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoTravel :: (TravelInfo -> Effect Unit) -> Effect Unit

-- Complete World Simulation mode FFI
foreign import mountWorldView :: Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> String -> (WorldInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import createWorld :: Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Int -> Boolean -> (WorldInfo -> Effect Unit) -> Effect Unit
foreign import stepWorld :: (WorldInfo -> Effect Unit) -> Effect Unit
foreign import advanceWorldStage :: (WorldInfo -> Effect Unit) -> Effect Unit
foreign import toggleWorldRunning :: (WorldInfo -> Effect Unit) -> Effect Unit
foreign import setWorldAspect :: String -> Effect Unit
foreign import setWorldSunAndAtmosphere :: Int -> Int -> (WorldInfo -> Effect Unit) -> Effect Unit
foreign import saveWorldPlanet :: (WorldInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoWorld :: (WorldInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoWorld :: (WorldInfo -> Effect Unit) -> Effect Unit

-- Moons & Tidal Erosion Sandbox mode FFI (`MoonScreen`)
foreign import mountMoonView :: Int -> Int -> Int -> Int -> String -> Boolean -> Boolean -> Boolean -> (MoonInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import setMoonParams :: Int -> Int -> Int -> Int -> (MoonInfo -> Effect Unit) -> Effect Unit
foreign import setMoonDisplayOptions :: String -> Boolean -> Boolean -> Boolean -> Effect Unit
foreign import stepMoonErode :: (MoonInfo -> Effect Unit) -> Effect Unit
foreign import resetMoonLayers :: (MoonInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoMoon :: (MoonInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoMoon :: (MoonInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoMoon :: (MoonInfo -> Effect Unit) -> Effect Unit

-- Meteors, Crater Excavation & Impact Winter Sandbox mode FFI (`MeteorScreen`)
foreign import mountMeteorView :: Number -> Number -> Number -> String -> Boolean -> Boolean -> (MeteorInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import setMeteorParams :: Number -> Number -> Number -> (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import setMeteorDisplayOptions :: String -> Boolean -> Boolean -> Effect Unit
foreign import strikeMeteorTile :: Boolean -> (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import strikeMeteorBarrage :: (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import stepMeteorRecover :: (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import resetMeteorPlanet :: (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import loadEarthIntoMeteor :: (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import loadSavedPlanetIntoMeteor :: (MeteorInfo -> Effect Unit) -> Effect Unit
foreign import loadFilePlanetIntoMeteor :: (MeteorInfo -> Effect Unit) -> Effect Unit

-- Sun, Plausible Galaxy & Celestial Sphere Star Map mode FFI (`SunScreen`)
foreign import mountSunView :: Int -> Int -> Int -> String -> Boolean -> Boolean -> (SunInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import generateSunGalaxy :: Int -> Int -> Int -> (SunInfo -> Effect Unit) -> Effect Unit
foreign import chooseNewSunInGalaxy :: Int -> (SunInfo -> Effect Unit) -> Effect Unit
foreign import loadHipparcosIntoSun :: (SunInfo -> Effect Unit) -> Effect Unit
foreign import setSunAtmosphere :: Int -> (SunInfo -> Effect Unit) -> Effect Unit
foreign import setSunDisplayOptions :: String -> Boolean -> Boolean -> Effect Unit

-- Shared viewport rotation FFI
foreign import rotateGrid :: Number -> Effect Unit

mkApp :: Component Unit
mkApp = do
  component "Tec11App" \_ -> React.do
    wasmReady /\ setWasmReady <- useState' false
    screen /\ setScreen <- useState' MainMenu
    rotationDeg /\ setRotationDeg <- useState' 0

    -- Grid mode state
    gridInfo /\ setGridInfo <- useState' { size: 0, tileCount: 12 }

    -- Pangea mode state
    planetSize /\ setPlanetSize <- useState' 6
    targetLandPct /\ setTargetLandPct <- useState' 29
    renderMode /\ setRenderMode <- useState' "color"
    pangeaInfo /\ setPangeaInfo <- useState' { gridSize: 6, tileCount: 7292, actualLandPercent: 29 }

    -- Split mode state
    splitSize /\ setSplitSize <- useState' 6
    splitTime /\ setSplitTimeState <- useState' 2
    splitInfo /\ setSplitInfo <- useState'
      { gridSize: 6
      , tileCount: 7292
      , iterations: 1
      , redCount: 1057
      , blueCount: 1057
      }

    -- Move mode state
    moveSize /\ setMoveSize <- useState' 6
    moveDirection /\ setMoveDirectionState <- useState' 45
    moveSpeed /\ setMoveSpeedState <- useState' 2
    moveTrail /\ setMoveTrailState <- useState' true
    moveInfo /\ setMoveInfo <- useState'
      { gridSize: 6
      , tileCount: 7292
      , groupTileCount: 1823
      , steps: 0
      , isRunning: false
      }

    -- Erosion mode state
    erosionShowLost /\ setErosionShowLost <- useState' true
    erosionShowGained /\ setErosionShowGained <- useState' true
    erosionInfo /\ setErosionInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , steps: 0
      , hasSavedPlanet: false
      , selectedTileReport: "Click any tile on the globe to inspect elevation, erosion, and deposition."
      }

    -- Climate mode state
    climateAttribute /\ setClimateAttributeState <- useState' "koeppen"
    climateSeason /\ setClimateSeasonState <- useState' 0
    climateGlaciation /\ setClimateGlaciationState <- useState' 50
    climateCells /\ setClimateCellsState <- useState' 3
    climateInfo /\ setClimateInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , hasSavedPlanet: false
      }

    -- Tectonics mode state
    tectonicsSize /\ setTectonicsSize <- useState' 5
    tectonicsLandPct /\ setTectonicsLandPct <- useState' 29
    tectonicsAspect /\ setTectonicsAspectState <- useState' "elevation"
    tectonicsInfo /\ setTectonicsInfo <- useState'
      { gridSize: 5
      , tileCount: 2432
      , geoticks: 0
      , continentCount: 1
      , actualLandPercent: 29
      , hasAtmosphere: false
      , hasLife: false
      , isRunning: false
      , selectedTileReport: "Click any tile on the globe to inspect its rock layers."
      }

    -- Rivers mode state (`hmin` slider 0..100 -> 0.0..10.0 km, `pmin` slider 0..100 -> 0.0..3.0)
    riversHmin /\ setRiversHmin <- useState' 50
    riversPmin /\ setRiversPmin <- useState' 17
    riversInfo /\ setRiversInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , riverCount: 0
      , hasSavedPlanet: false
      }

    -- Lifeforms mode state
    lifeformsAttribute /\ setLifeformsAttributeState <- useState' "life"
    lifeformsSeason /\ setLifeformsSeasonState <- useState' 0
    lifeformsGlaciation /\ setLifeformsGlaciationState <- useState' 50
    lifeformsInfo /\ setLifeformsInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , settledAnimals: 0
      , settledPlants: 0
      , hasSavedPlanet: false
      , selectedTileIdx: -1
      , selectedTileRegion: ""
      , selectedSpeciesId: -1
      , tileSpecies: []
      }

    -- Population mode state
    populationCoastProx /\ setPopulationCoastProxState <- useState' 2
    populationTravelRange /\ setPopulationTravelRangeState <- useState' 6
    populationInfo /\ setPopulationInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , populatedCount: 0
      , ticks: 0
      , isRunning: false
      , hasSavedPlanet: false
      }

    -- Racination mode state
    racinationTravelRange /\ setRacinationTravelRangeState <- useState' 6
    racinationInfo /\ setRacinationInfo <- useState'
      { tileCount: 7292
      , peoples: 1
      , populatedCount: 0
      , isolations: 0
      }

    -- Prehistory mode state
    prehistoryInfo /\ setPrehistoryInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , ticks: 0
      , kyr: 0
      , glaciationPct: 50
      , races: 0
      , agRaces: 0
      , populatedCount: 0
      , populationText: "0"
      , isRunning: false
      , hasSavedPlanet: false
      }

    -- Language mode state
    languageInfo /\ setLanguageInfo <- useState'
      { sourcePhonemes: ""
      , destPhonemes: ""
      , words: []
      }

    -- Terrain mode state (`terrainpresenter.py`)
    terrainShowRivers /\ setTerrainShowRiversState <- useState' true
    terrainInfo /\ setTerrainInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , displayFaceCount: 7292
      , fineFaceCount: 0
      , coarseTileCount: 7292
      , riverCount: 0
      , hasSavedPlanet: false
      , selectedReport: "Click any terrain face to inspect its elevation, Simplex noise perturbation, and parent coarse tile interpolation."
      }

    -- History mode state
    historyAspect /\ setHistoryAspectState <- useState' "nations"
    historyShowRivers /\ setHistoryShowRiversState <- useState' true
    historyInfo /\ setHistoryInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , ticks: 0
      , years: 0
      , activeStates: 0
      , populationText: "0"
      , isRunning: false
      , hasSavedPlanet: false
      , selectedReport: "Click any state on the globe to inspect its demographics, diplomacy, and language lexicon."
      }

    -- Travel mode state
    accGeoOpen /\ setAccGeoOpen <- useState' true
    accBioOpen /\ setAccBioOpen <- useState' true
    accHistOpen /\ setAccHistOpen <- useState' true
    accAstroOpen /\ setAccAstroOpen <- useState' true
    travelAspect /\ setTravelAspectState <- useState' "travel"
    travelShowPotential /\ setTravelShowPotential <- useState' false
    travelShowWindsCurrents /\ setTravelShowWindsCurrents <- useState' true
    travelShowRivers /\ setTravelShowRivers <- useState' true
    travelInfo /\ setTravelInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , ticks: 0
      , years: 0
      , activeStates: 0
      , populationText: "0"
      , activeRoutes: 0
      , potentialRoutes: 0
      , portCount: 0
      , tradeRoutes: 0
      , conflictRoutes: 0
      , isRunning: false
      , hasSavedPlanet: false
      , selectedReport: "Click any port, ocean current, or coastal state on the globe to inspect wind & current vectors, maritime sea lanes, and riverine corridors."
      }

    -- Complete World Simulation state (`world.py`, `worldpresenter.py`)
    worldSize /\ setWorldSize <- useState' 5
    worldDayHours /\ setWorldDayHours <- useState' 24
    worldTiltDeg /\ setWorldTiltDeg <- useState' 23
    worldLandPct /\ setWorldLandPct <- useState' 29
    worldAtmDt /\ setWorldAtmDt <- useState' 125
    worldLifeDt /\ setWorldLifeDt <- useState' 125
    worldPeopleDt /\ setWorldPeopleDt <- useState' 125
    worldMoonCount /\ setWorldMoonCount <- useState' 1
    worldMoonMassTenths /\ setWorldMoonMassTenths <- useState' 10
    worldMoonDistRadii /\ setWorldMoonDistRadii <- useState' 60
    worldMeteorFreqTenths /\ setWorldMeteorFreqTenths <- useState' 10
    worldSunFilter /\ setWorldSunFilter <- useState' 2
    worldAtmosphereId /\ setWorldAtmosphereId <- useState' 0
    worldRandomize /\ setWorldRandomize <- useState' true
    worldAspect /\ setWorldAspectState <- useState' "color"
    worldInfo /\ setWorldInfo <- useState'
      { stageIndex: 0
      , stageName: "Tectonics (Hadean / Early Crust)"
      , tileCount: 2432
      , yearsText: "0"
      , populationText: "0"
      , gridSize: 5
      , dayHours: 24
      , tiltDeg: 23
      , landPct: 29
      , atmDtMyr: 125
      , lifeDtMyr: 125
      , peopleDtMyr: 125
      , moonCount: 1
      , moonMassTenths: 10
      , moonDistRadii: 60
      , meteorFreqTenths: 10
      , orbitalSummaryText: "Day: 11.1h (→24h) | Tilt: 23.0° | Moons: 1 (1.0x @ 23.4→60 Rp, Tides 14.5x) | Meteors: 1.0x (0 hits)"
      , sunFilter: 2
      , atmosphereId: 0
      , sunSpectralType: "G2V"
      , sunColorCss: "rgb(255, 255, 119)"
      , atmosphereName: "N2-O2 + O3 Ozone (Earth-like)"
      , skyColorCss: "rgb(41, 130, 240)"
      , skyColorName: "Azure Blue"
      , sunsetColorCss: "rgb(250, 97, 36)"
      , sunsetColorName: "Crimson-Orange & Violet-Pink"
      , plantColorCss: "rgb(36, 138, 41)"
      , plantColorName: "Emerald Chlorophyll Green"
      , planetsSummaryText: "#1: Scorched Rocky (0.38 R⊕ @ 0.39 AU) | #2: Cloudy Rocky (0.95 R⊕ @ 0.72 AU) | #3 [HOME]: Temperate Rocky (1.00 R⊕ @ 1.00 AU)"
      , isRunning: false
      , hasSavedPlanet: false
      , selectedReport: "Click any tile on the globe to inspect its rock strata, climate classification, and population / heritage ancestry tree."
      }

    -- Moons & Tidal Erosion Sandbox state (`MoonScreen`)
    moonCount /\ setMoonCountState <- useState' 1
    moonMassTenths /\ setMoonMassTenthsState <- useState' 10
    moonDistRadii /\ setMoonDistRadiiState <- useState' 60
    moonEpochPct /\ setMoonEpochPctState <- useState' 100
    moonAspect /\ setMoonAspectState <- useState' "tidal_erosion"
    moonShowTidalScour /\ setMoonShowTidalScour <- useState' true
    moonShowFluvial /\ setMoonShowFluvial <- useState' true
    moonShowDeposition /\ setMoonShowDeposition <- useState' true
    moonInfo /\ setMoonInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , steps: 0
      , harmonicSummary: "Day: 24.0h (→24h) | Tilt: 23.0° | Tides: 1.00x Earth | M1: 60.0 Rp (1:1, 100% tide)"
      , tidalSharePctText: "0.0%"
      , meanCoastalScourText: "0.0000 km/tick"
      , maxTidalRangeText: "0.0 m"
      , hasSavedPlanet: false
      , selectedReport: "Click any coastal, ocean, or inland tile on the globe to inspect local tidal range (m), tidal scour vs. rainfall erosion rates, shelf deposition, and rock strata."
      }

    -- Meteors, Crater Excavation & Impact Winter Sandbox state (`MeteorScreen`)
    meteorDiameterKm /\ setMeteorDiameterKm <- useState' 10
    meteorVelocityKmS /\ setMeteorVelocityKmS <- useState' 20
    meteorDensityKgM3 /\ setMeteorDensityKgM3 <- useState' 3000
    meteorAspect /\ setMeteorAspectState <- useState' "crater_ejecta"
    meteorShowEjecta /\ setMeteorShowEjecta <- useState' true
    meteorShowBlast /\ setMeteorShowBlast <- useState' true
    meteorInfo /\ setMeteorInfo <- useState'
      { planetName: "Earth (ETOPO2)"
      , tileCount: 7292
      , impacts: 0
      , recoverySteps: 0
      , impactSummary: "Impactor: 10 km @ 20 km/s → Crater 186 km (75.1M MT, Chicxulub Extinction Class) | Cooling: -0.0 °C | Biosphere: 100%"
      , coolingText: "-0.0 °C"
      , biospherePctText: "100%"
      , hasSavedPlanet: false
      , selectedReport: "Click any tile on the globe to target it for a meteor strike and inspect crater excavation, ejecta breccia, impact winter cooling, and rock strata."
      }

    -- Sun, Plausible Galaxy & Celestial Sphere Star Map state (`SunScreen`)
    sunGalaxyType /\ setSunGalaxyType <- useState' 99
    sunVantage /\ setSunVantage <- useState' 99
    sunStarFilter /\ setSunStarFilter <- useState' 0
    sunAtmosphere /\ setSunAtmosphereState <- useState' 0
    sunAspect /\ setSunAspectState <- useState' "night_sky"
    sunShowGalacticPlane /\ setSunShowGalacticPlane <- useState' true
    sunShowConstellations /\ setSunShowConstellations <- useState' false
    sunInfo /\ setSunInfo <- useState'
      { galaxyTypeName: "Spiral Galaxy (Sb/Sc)"
      , vantageName: "Spiral Arm (Star-Forming Disk)"
      , starCount: 6000
      , sunSpectralType: "G2V"
      , sunClassDesc: "Yellow Main-Sequence Dwarf"
      , sunColorCss: "rgb(255, 255, 119)"
      , sunBvText: "+0.65"
      , sunTempText: "5778 K"
      , sunRadiusText: "1.00 R☉"
      , sunMassText: "1.00 M☉"
      , sunLumText: "1.00 L☉"
      , sunHzText: "1.00 AU (365d yr)"
      , atmosphereName: "N2-O2 + O3 Ozone (Earth-like)"
      , atmosphereFormula: "78% N2, 21% O2, 1% Ar + O3"
      , skyColorCss: "rgb(41, 130, 240)"
      , skyHorizonCss: "rgb(138, 194, 250)"
      , skyColorName: "Azure Blue"
      , skyMechanism: "Rayleigh scattering (lambda^-4) by N2/O2 + O3 Chappuis band absorption"
      , sunsetColorCss: "rgb(250, 97, 36)"
      , twilightColorCss: "rgb(117, 51, 133)"
      , sunsetColorName: "Crimson-Orange & Violet-Pink"
      , sunsetMechanism: "Long slant-path Rayleigh blue depletion + O3 Chappuis twilight purple"
      , plantColorCss: "rgb(36, 138, 41)"
      , plantHighlightCss: "rgb(82, 189, 71)"
      , plantColorName: "Emerald Chlorophyll Green"
      , plantPigmentDesc: "Chlorophyll a/b absorbs abundant 430-470 nm blue & 640-680 nm red photons while reflecting 500-560 nm green peak to prevent photo-oxidative overheating"
      , planetsSummaryText: "#1: Scorched Rocky (0.38 R⊕ @ 0.39 AU) | #2: Cloudy Rocky (0.95 R⊕ @ 0.72 AU) | #3 [HOME]: Temperate Rocky (1.00 R⊕ @ 1.00 AU)"
      , summaryLine: "Sun: G2V (5778 K, B-V +0.65, 1.00 R☉, 1.00 L☉) | Sky: Azure Blue | Sunset: Crimson-Orange & Violet-Pink | Plants: Emerald Chlorophyll Green"
      , selectedReport: "Click any star or planetary tile on the sphere to inspect its spectral class, apparent & absolute magnitude, distance in light-years, atmospheric scattering, sunset optical depth, and photosynthetic plant pigments."
      }

    useEffectOnce do
      initWasm (setWasmReady true)
      pure (pure unit)

    useEffect screen do
      case screen of
        MainMenu -> pure (pure unit)
        GridScreen -> do
          setRotationDeg 0
          mountGridView setGridInfo
        PangeaScreen -> do
          setRotationDeg 0
          mountPangeaView planetSize targetLandPct renderMode setPangeaInfo
        SplitScreen -> do
          setRotationDeg 0
          mountSplitView splitSize splitTime setSplitInfo
        MoveScreen -> do
          setRotationDeg 0
          mountMoveView moveSize (Int.toNumber moveDirection) (Int.toNumber moveSpeed) moveTrail setMoveInfo
        ErosionScreen -> do
          setRotationDeg 0
          mountErosionView erosionShowLost erosionShowGained setErosionInfo
        ClimateScreen -> do
          setRotationDeg 0
          mountClimateView climateCells climateGlaciation climateAttribute climateSeason setClimateInfo
        TectonicsScreen -> do
          setRotationDeg 0
          mountTectonicsView tectonicsSize tectonicsLandPct tectonicsAspect setTectonicsInfo
        RiversScreen -> do
          setRotationDeg 0
          mountRiversView riversHmin riversPmin setRiversInfo
        LifeformsScreen -> do
          setRotationDeg 0
          mountLifeformsView lifeformsGlaciation lifeformsAttribute lifeformsSeason setLifeformsInfo
        PopulationScreen -> do
          setRotationDeg 0
          mountPopulationView populationCoastProx populationTravelRange setPopulationInfo
        RacinationScreen -> do
          setRotationDeg 0
          mountRacinationView racinationTravelRange setRacinationInfo
        PrehistoryScreen -> do
          setRotationDeg 0
          mountPrehistoryView setPrehistoryInfo
        LanguageScreen -> do
          mountLanguageView setLanguageInfo
        TerrainScreen -> do
          setRotationDeg 0
          mountTerrainView terrainShowRivers setTerrainInfo
        TravelScreen -> do
          setRotationDeg 0
          mountTravelView travelAspect travelShowPotential travelShowWindsCurrents travelShowRivers setTravelInfo
        HistoryScreen -> do
          setRotationDeg 0
          mountHistoryView historyAspect historyShowRivers setHistoryInfo
        WorldScreen -> do
          setRotationDeg 0
          mountWorldView worldSize worldDayHours worldTiltDeg worldLandPct worldAtmDt worldLifeDt worldPeopleDt worldMoonCount worldMoonMassTenths worldMoonDistRadii worldMeteorFreqTenths worldAspect setWorldInfo
        MoonScreen -> do
          setRotationDeg 0
          mountMoonView moonCount moonMassTenths moonDistRadii moonEpochPct moonAspect moonShowTidalScour moonShowFluvial moonShowDeposition setMoonInfo
        MeteorScreen -> do
          setRotationDeg 0
          mountMeteorView (Int.toNumber meteorDiameterKm) (Int.toNumber meteorVelocityKmS) (Int.toNumber meteorDensityKgM3) meteorAspect meteorShowEjecta meteorShowBlast setMeteorInfo
        SunScreen -> do
          setRotationDeg 0
          mountSunView sunGalaxyType sunVantage sunStarFilter sunAspect sunShowGalacticPlane sunShowConstellations setSunInfo

    let
      handleRotateChange :: Maybe String -> Effect Unit
      handleRotateChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just deg -> do
          setRotationDeg deg
          rotateGrid (Int.toNumber deg)

      handlePlanetSizeChange :: Maybe String -> Effect Unit
      handlePlanetSizeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just sz -> do
          setPlanetSize sz
          regeneratePangea sz targetLandPct setPangeaInfo

      handleLandPctChange :: Maybe String -> Effect Unit
      handleLandPctChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just pct -> do
          setTargetLandPct pct
          regeneratePangea planetSize pct setPangeaInfo

      handleRenderModeChange :: Maybe String -> Effect Unit
      handleRenderModeChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just mode -> do
          setRenderMode mode
          setPangeaRenderMode mode

      handleGeneratePangea :: Effect Unit
      handleGeneratePangea =
        regeneratePangea planetSize targetLandPct setPangeaInfo

      handleSplitTimeChange :: Maybe String -> Effect Unit
      handleSplitTimeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just t -> do
          setSplitTimeState t
          setSplitTime t

      handleSplitSizeChange :: Maybe String -> Effect Unit
      handleSplitSizeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just sz -> do
          setSplitSize sz
          resetSplit sz setSplitInfo

      handleMoveDirectionChange :: Maybe String -> Effect Unit
      handleMoveDirectionChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just dir -> do
          setMoveDirectionState dir
          setMoveDirection (Int.toNumber dir)

      handleMoveSpeedChange :: Maybe String -> Effect Unit
      handleMoveSpeedChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just spd -> do
          setMoveSpeedState spd
          setMoveSpeed (Int.toNumber spd)

      handleMoveSizeChange :: Maybe String -> Effect Unit
      handleMoveSizeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just sz -> do
          setMoveSize sz
          resetMove sz (Int.toNumber moveDirection) (Int.toNumber moveSpeed) setMoveInfo

      handleToggleMoveTrail :: Effect Unit
      handleToggleMoveTrail = do
        let next = not moveTrail
        setMoveTrailState next
        setMoveTrail next

      handleToggleErosionLost :: Effect Unit
      handleToggleErosionLost = do
        let next = not erosionShowLost
        setErosionShowLost next
        setErosionOverlays next erosionShowGained

      handleToggleErosionGained :: Effect Unit
      handleToggleErosionGained = do
        let next = not erosionShowGained
        setErosionShowGained next
        setErosionOverlays erosionShowLost next

      handleClimateAttrChange :: Maybe String -> Effect Unit
      handleClimateAttrChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just attr -> do
          setClimateAttributeState attr
          setClimateAttribute attr

      handleClimateSeasonChange :: Maybe String -> Effect Unit
      handleClimateSeasonChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just s -> do
          setClimateSeasonState s
          setClimateSeason s

      handleClimateGlaciationChange :: Maybe String -> Effect Unit
      handleClimateGlaciationChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just g -> do
          setClimateGlaciationState g
          setClimateParams climateCells g

      handleClimateCellsChange :: Maybe String -> Effect Unit
      handleClimateCellsChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just c -> do
          setClimateCellsState c
          setClimateParams c climateGlaciation

      handleTectonicsAspectChange :: Maybe String -> Effect Unit
      handleTectonicsAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just aspect -> do
          setTectonicsAspectState aspect
          setTectonicsAspect aspect

      handleTectonicsSizeChange :: Maybe String -> Effect Unit
      handleTectonicsSizeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just sz -> do
          setTectonicsSize sz
          resetTectonics sz tectonicsLandPct setTectonicsInfo

      handleTectonicsLandPctChange :: Maybe String -> Effect Unit
      handleTectonicsLandPctChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just pct -> do
          setTectonicsLandPct pct

      handleRiversHminChange :: Maybe String -> Effect Unit
      handleRiversHminChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just h -> do
          setRiversHmin h
          setRiversParams h riversPmin setRiversInfo

      handleRiversPminChange :: Maybe String -> Effect Unit
      handleRiversPminChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just p -> do
          setRiversPmin p
          setRiversParams riversHmin p setRiversInfo

      handleLifeformsAttrChange :: Maybe String -> Effect Unit
      handleLifeformsAttrChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just attr -> do
          setLifeformsAttributeState attr
          setLifeformsAttribute attr setLifeformsInfo

      handleLifeformsSeasonChange :: Maybe String -> Effect Unit
      handleLifeformsSeasonChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just s -> do
          setLifeformsSeasonState s
          setLifeformsSeason s setLifeformsInfo

      handleLifeformsGlaciationChange :: Maybe String -> Effect Unit
      handleLifeformsGlaciationChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just g -> do
          setLifeformsGlaciationState g
          setLifeformsGlaciation g setLifeformsInfo

      handlePopulationCoastProxChange :: Maybe String -> Effect Unit
      handlePopulationCoastProxChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just cp -> do
          setPopulationCoastProxState cp
          setPopulationCoastProx cp

      handlePopulationTravelRangeChange :: Maybe String -> Effect Unit
      handlePopulationTravelRangeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just tr -> do
          setPopulationTravelRangeState tr
          setPopulationTravelRange tr

      handleRacinationTravelRangeChange :: Maybe String -> Effect Unit
      handleRacinationTravelRangeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just tr -> do
          setRacinationTravelRangeState tr
          setRacinationTravelRange tr

      handleToggleTerrainRivers :: Effect Unit
      handleToggleTerrainRivers = do
        let next = not terrainShowRivers
        setTerrainShowRiversState next
        setTerrainShowRivers next

      handleHistoryAspectChange :: Maybe String -> Effect Unit
      handleHistoryAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just asp -> do
          setHistoryAspectState asp
          setHistoryAspect asp

      handleToggleHistoryRivers :: Effect Unit
      handleToggleHistoryRivers = do
        let next = not historyShowRivers
        setHistoryShowRiversState next
        setHistoryShowRivers next

      handleTravelAspectChange :: Maybe String -> Effect Unit
      handleTravelAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just asp -> do
          setTravelAspectState asp
          setTravelDisplayOptions asp travelShowPotential travelShowWindsCurrents travelShowRivers

      handleToggleTravelPotential :: Effect Unit
      handleToggleTravelPotential = do
        let next = not travelShowPotential
        setTravelShowPotential next
        setTravelDisplayOptions travelAspect next travelShowWindsCurrents travelShowRivers

      handleToggleTravelWindsCurrents :: Effect Unit
      handleToggleTravelWindsCurrents = do
        let next = not travelShowWindsCurrents
        setTravelShowWindsCurrents next
        setTravelDisplayOptions travelAspect travelShowPotential next travelShowRivers

      handleToggleTravelRivers :: Effect Unit
      handleToggleTravelRivers = do
        let next = not travelShowRivers
        setTravelShowRivers next
        setTravelDisplayOptions travelAspect travelShowPotential travelShowWindsCurrents next

      handleWorldAspectChange :: Maybe String -> Effect Unit
      handleWorldAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just asp -> do
          setWorldAspectState asp
          setWorldAspect asp

      handleWorldSizeChange :: Maybe String -> Effect Unit
      handleWorldSizeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just sz -> setWorldSize sz

      handleWorldDayHoursChange :: Maybe String -> Effect Unit
      handleWorldDayHoursChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just dh -> setWorldDayHours dh

      handleWorldTiltChange :: Maybe String -> Effect Unit
      handleWorldTiltChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just t -> setWorldTiltDeg t

      handleWorldLandChange :: Maybe String -> Effect Unit
      handleWorldLandChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just l -> setWorldLandPct l

      handleWorldAtmDtChange :: Maybe String -> Effect Unit
      handleWorldAtmDtChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just dt -> setWorldAtmDt dt

      handleWorldLifeDtChange :: Maybe String -> Effect Unit
      handleWorldLifeDtChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just dt -> setWorldLifeDt dt

      handleWorldPeopleDtChange :: Maybe String -> Effect Unit
      handleWorldPeopleDtChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just dt -> setWorldPeopleDt dt

      handleWorldMoonCountChange :: Maybe String -> Effect Unit
      handleWorldMoonCountChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just mc -> setWorldMoonCount mc

      handleWorldMoonMassChange :: Maybe String -> Effect Unit
      handleWorldMoonMassChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just mm -> setWorldMoonMassTenths mm

      handleWorldMoonDistChange :: Maybe String -> Effect Unit
      handleWorldMoonDistChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just md -> setWorldMoonDistRadii md

      handleWorldMeteorFreqChange :: Maybe String -> Effect Unit
      handleWorldMeteorFreqChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just mf -> setWorldMeteorFreqTenths mf

      handleWorldSunFilterChange :: Maybe String -> Effect Unit
      handleWorldSunFilterChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just sf -> do
          setWorldSunFilter sf
          setWorldSunAndAtmosphere sf worldAtmosphereId setWorldInfo

      handleWorldAtmosphereChange :: Maybe String -> Effect Unit
      handleWorldAtmosphereChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just atm -> do
          setWorldAtmosphereId atm
          setWorldSunAndAtmosphere worldSunFilter atm setWorldInfo

      handleToggleWorldRandomize :: Effect Unit
      handleToggleWorldRandomize =
        setWorldRandomize (not worldRandomize)

      handleCreateWorld :: Effect Unit
      handleCreateWorld =
        createWorld worldSize worldDayHours worldTiltDeg worldLandPct worldAtmDt worldLifeDt worldPeopleDt worldMoonCount worldMoonMassTenths worldMoonDistRadii worldMeteorFreqTenths worldRandomize \info -> do
          setWorldInfo info
          setWorldDayHours info.dayHours
          setWorldTiltDeg info.tiltDeg
          setWorldLandPct info.landPct
          setWorldAtmDt info.atmDtMyr
          setWorldLifeDt info.lifeDtMyr
          setWorldPeopleDt info.peopleDtMyr
          setWorldMoonCount info.moonCount
          setWorldMoonMassTenths info.moonMassTenths
          setWorldMoonDistRadii info.moonDistRadii
          setWorldMeteorFreqTenths info.meteorFreqTenths
          setWorldSunFilter info.sunFilter
          setWorldAtmosphereId info.atmosphereId

      handleMoonCountChange :: Maybe String -> Effect Unit
      handleMoonCountChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just mc -> do
          setMoonCountState mc
          setMoonParams mc moonMassTenths moonDistRadii moonEpochPct setMoonInfo

      handleMoonMassChange :: Maybe String -> Effect Unit
      handleMoonMassChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just mm -> do
          setMoonMassTenthsState mm
          setMoonParams moonCount mm moonDistRadii moonEpochPct setMoonInfo

      handleMoonDistChange :: Maybe String -> Effect Unit
      handleMoonDistChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just md -> do
          setMoonDistRadiiState md
          setMoonParams moonCount moonMassTenths md moonEpochPct setMoonInfo

      handleMoonEpochChange :: Maybe String -> Effect Unit
      handleMoonEpochChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just ep -> do
          setMoonEpochPctState ep
          setMoonParams moonCount moonMassTenths moonDistRadii ep setMoonInfo

      handleMoonAspectChange :: Maybe String -> Effect Unit
      handleMoonAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just asp -> do
          setMoonAspectState asp
          setMoonDisplayOptions asp moonShowTidalScour moonShowFluvial moonShowDeposition

      handleToggleMoonTidalScour :: Effect Unit
      handleToggleMoonTidalScour = do
        let next = not moonShowTidalScour
        setMoonShowTidalScour next
        setMoonDisplayOptions moonAspect next moonShowFluvial moonShowDeposition

      handleToggleMoonFluvial :: Effect Unit
      handleToggleMoonFluvial = do
        let next = not moonShowFluvial
        setMoonShowFluvial next
        setMoonDisplayOptions moonAspect moonShowTidalScour next moonShowDeposition

      handleToggleMoonDeposition :: Effect Unit
      handleToggleMoonDeposition = do
        let next = not moonShowDeposition
        setMoonShowDeposition next
        setMoonDisplayOptions moonAspect moonShowTidalScour moonShowFluvial next

      handleMeteorDiameterChange :: Maybe String -> Effect Unit
      handleMeteorDiameterChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just d -> do
          setMeteorDiameterKm d
          setMeteorParams (Int.toNumber d) (Int.toNumber meteorVelocityKmS) (Int.toNumber meteorDensityKgM3) setMeteorInfo

      handleMeteorVelocityChange :: Maybe String -> Effect Unit
      handleMeteorVelocityChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just v -> do
          setMeteorVelocityKmS v
          setMeteorParams (Int.toNumber meteorDiameterKm) (Int.toNumber v) (Int.toNumber meteorDensityKgM3) setMeteorInfo

      handleMeteorDensityChange :: Maybe String -> Effect Unit
      handleMeteorDensityChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just rho -> do
          setMeteorDensityKgM3 rho
          setMeteorParams (Int.toNumber meteorDiameterKm) (Int.toNumber meteorVelocityKmS) (Int.toNumber rho) setMeteorInfo

      handleMeteorAspectChange :: Maybe String -> Effect Unit
      handleMeteorAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just asp -> do
          setMeteorAspectState asp
          setMeteorDisplayOptions asp meteorShowEjecta meteorShowBlast

      handleToggleMeteorEjecta :: Effect Unit
      handleToggleMeteorEjecta = do
        let next = not meteorShowEjecta
        setMeteorShowEjecta next
        setMeteorDisplayOptions meteorAspect next meteorShowBlast

      handleToggleMeteorBlast :: Effect Unit
      handleToggleMeteorBlast = do
        let next = not meteorShowBlast
        setMeteorShowBlast next
        setMeteorDisplayOptions meteorAspect meteorShowEjecta next

      handleSunGalaxyTypeChange :: Maybe String -> Effect Unit
      handleSunGalaxyTypeChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just g -> do
          setSunGalaxyType g
          generateSunGalaxy g sunVantage sunStarFilter setSunInfo

      handleSunVantageChange :: Maybe String -> Effect Unit
      handleSunVantageChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just v -> do
          setSunVantage v
          generateSunGalaxy sunGalaxyType v sunStarFilter setSunInfo

      handleSunStarFilterChange :: Maybe String -> Effect Unit
      handleSunStarFilterChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just f -> do
          setSunStarFilter f
          chooseNewSunInGalaxy f setSunInfo

      handleSunAtmosphereChange :: Maybe String -> Effect Unit
      handleSunAtmosphereChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just atm -> do
          setSunAtmosphereState atm
          setSunAtmosphere atm setSunInfo

      handleSunAspectChange :: Maybe String -> Effect Unit
      handleSunAspectChange maybeVal = case maybeVal of
        Nothing -> pure unit
        Just asp -> do
          setSunAspectState asp
          setSunDisplayOptions asp sunShowGalacticPlane sunShowConstellations

      handleToggleSunGalacticPlane :: Effect Unit
      handleToggleSunGalacticPlane = do
        let next = not sunShowGalacticPlane
        setSunShowGalacticPlane next
        setSunDisplayOptions sunAspect next sunShowConstellations

      handleToggleSunConstellations :: Effect Unit
      handleToggleSunConstellations = do
        let next = not sunShowConstellations
        setSunShowConstellations next
        setSunDisplayOptions sunAspect sunShowGalacticPlane next

    pure case screen of
      MainMenu ->
        R.div
          { className: "main-menu"
          , children:
              [ R.h1_ [ R.text "Tec11" ]
              , R.div
                  { className: "menu-buttons"
                  , children:
                      [ R.button
                          { id: "world-btn"
                          , className: "menu-btn"
                          , disabled: not wasmReady
                          , onClick: capture_ (setScreen WorldScreen)
                          , children: [ R.text "Start (World Simulation)" ]
                          }
                      , R.button
                          { id: "grid-btn"
                          , className: "menu-btn"
                          , disabled: not wasmReady
                          , onClick: capture_ (setScreen GridScreen)
                          , children: [ R.text "Grid" ]
                          }
                      , R.div
                          { className: "menu-accordion"
                          , children:
                              ( [ R.button
                                    { className: "menu-accordion-header"
                                    , onClick: capture_ (setAccGeoOpen (not accGeoOpen))
                                    , children:
                                        [ R.span { children: [ R.text "Continents" ] }
                                        , R.span { children: [ R.text (if accGeoOpen then "▼" else "▶") ] }
                                        ]
                                    }
                                ]
                                  <>
                                    if accGeoOpen then
                                      [ R.div
                                          { className: "menu-accordion-body"
                                          , children:
                                              [ R.button
                                                  { id: "pangea-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen PangeaScreen)
                                                  , children: [ R.text "Pangea" ]
                                                  }
                                              , R.button
                                                  { id: "split-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen SplitScreen)
                                                  , children: [ R.text "Split" ]
                                                  }
                                              , R.button
                                                  { id: "move-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen MoveScreen)
                                                  , children: [ R.text "Move" ]
                                                  }
                                              , R.button
                                                  { id: "tectonics-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen TectonicsScreen)
                                                  , children: [ R.text "Tectonics" ]
                                                  }
                                              , R.button
                                                  { id: "terrain-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen TerrainScreen)
                                                  , children: [ R.text "Terrain" ]
                                                  }
                                              ]
                                          }
                                      ]
                                    else
                                      []
                              )
                          }
                      , R.div
                          { className: "menu-accordion"
                          , children:
                              ( [ R.button
                                    { className: "menu-accordion-header"
                                    , onClick: capture_ (setAccBioOpen (not accBioOpen))
                                    , children:
                                        [ R.span { children: [ R.text "Weather" ] }
                                        , R.span { children: [ R.text (if accBioOpen then "▼" else "▶") ] }
                                        ]
                                    }
                                ]
                                  <>
                                    if accBioOpen then
                                      [ R.div
                                          { className: "menu-accordion-body"
                                          , children:
                                              [ R.button
                                                  { id: "erosion-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen ErosionScreen)
                                                  , children: [ R.text "Erosion" ]
                                                  }
                                              , R.button
                                                  { id: "climate-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen ClimateScreen)
                                                  , children: [ R.text "Climate" ]
                                                  }
                                              , R.button
                                                  { id: "rivers-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen RiversScreen)
                                                  , children: [ R.text "Rivers" ]
                                                  }
                                              ]
                                          }
                                      ]
                                    else
                                      []
                              )
                          }
                      , R.div
                          { className: "menu-accordion"
                          , children:
                              ( [ R.button
                                    { className: "menu-accordion-header"
                                    , onClick: capture_ (setAccAstroOpen (not accAstroOpen))
                                    , children:
                                        [ R.span { children: [ R.text "Space" ] }
                                        , R.span { children: [ R.text (if accAstroOpen then "▼" else "▶") ] }
                                        ]
                                    }
                                ]
                                  <>
                                    if accAstroOpen then
                                      [ R.div
                                          { className: "menu-accordion-body"
                                          , children:
                                              [ R.button
                                                  { id: "sun-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen SunScreen)
                                                  , children: [ R.text "Sun" ]
                                                  }
                                              , R.button
                                                  { id: "moon-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen MoonScreen)
                                                  , children: [ R.text "Moons" ]
                                                  }
                                              , R.button
                                                  { id: "meteor-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen MeteorScreen)
                                                  , children: [ R.text "Meteors" ]
                                                  }
                                              ]
                                          }
                                      ]
                                    else
                                      []
                              )
                          }
                      , R.div
                          { className: "menu-accordion"
                          , children:
                              ( [ R.button
                                    { className: "menu-accordion-header"
                                    , onClick: capture_ (setAccHistOpen (not accHistOpen))
                                    , children:
                                        [ R.span { children: [ R.text "Life" ] }
                                        , R.span { children: [ R.text (if accHistOpen then "▼" else "▶") ] }
                                        ]
                                    }
                                ]
                                  <>
                                    if accHistOpen then
                                      [ R.div
                                          { className: "menu-accordion-body"
                                          , children:
                                              [ R.button
                                                  { id: "lifeforms-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen LifeformsScreen)
                                                  , children: [ R.text "Lifeforms" ]
                                                  }
                                              , R.button
                                                  { id: "population-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen PopulationScreen)
                                                  , children: [ R.text "Population" ]
                                                  }
                                              , R.button
                                                  { id: "racination-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen RacinationScreen)
                                                  , children: [ R.text "Racination" ]
                                                  }
                                              , R.button
                                                  { id: "prehistory-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen PrehistoryScreen)
                                                  , children: [ R.text "Prehistory" ]
                                                  }
                                              , R.button
                                                  { id: "language-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen LanguageScreen)
                                                  , children: [ R.text "Language" ]
                                                  }
                                              , R.button
                                                  { id: "travel-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen TravelScreen)
                                                  , children: [ R.text "Travel" ]
                                                  }
                                              , R.button
                                                  { id: "history-btn"
                                                  , className: "menu-btn"
                                                  , disabled: not wasmReady
                                                  , onClick: capture_ (setScreen HistoryScreen)
                                                  , children: [ R.text "History" ]
                                                  }
                                              ]
                                          }
                                      ]
                                    else
                                      []
                              )
                          }
                      ]
                  }
              ]
          }

      GridScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column"
                  , children:
                      [ R.div
                          { id: "grid-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "grid-size-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Size " <> show gridInfo.size) ]
                                  }
                              , R.div
                                  { id: "grid-tiles-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show gridInfo.tileCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "finer-btn"
                          , className: "action-btn"
                          , disabled: gridInfo.size >= 7
                          , onClick: capture_ (subdivideGrid setGridInfo)
                          , children: [ R.text "Finer" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      PangeaScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "render-mode-select"
                                  , className: "control-select"
                                  , value: renderMode
                                  , onChange: handler targetValue handleRenderModeChange
                                  , children:
                                      [ R.option
                                          { value: "color"
                                          , children: [ R.text "Color (Rock / Water)" ]
                                          }
                                      , R.option
                                          { value: "elevation"
                                          , children: [ R.text "Elevation" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Planet Size" ]
                                  }
                              , R.select
                                  { id: "planet-size-select"
                                  , className: "control-select"
                                  , value: show planetSize
                                  , onChange: handler targetValue handlePlanetSizeChange
                                  , children:
                                      [ R.option { value: "4", children: [ R.text "Very Small (2,100 km)" ] }
                                      , R.option { value: "5", children: [ R.text "Small (3,700 km)" ] }
                                      , R.option { value: "6", children: [ R.text "Medium / Earth (6,400 km)" ] }
                                      , R.option { value: "7", children: [ R.text "Large (11,000 km)" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text ("Continent Size (" <> show targetLandPct <> "% land)") ]
                                  }
                              , R.input
                                  { id: "land-percent-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "5"
                                  , max: "65"
                                  , step: "1"
                                  , value: show targetLandPct
                                  , onChange: handler targetValue handleLandPctChange
                                  }
                              ]
                          }
                      , R.div
                          { id: "pangea-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "pangea-land-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show pangeaInfo.actualLandPercent <> "% land") ]
                                  }
                              , R.div
                                  { id: "pangea-tiles-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show pangeaInfo.tileCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "generate-btn"
                          , className: "action-btn"
                          , onClick: capture_ handleGeneratePangea
                          , children: [ R.text "Generate" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      ClimateScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "climate-attribute-select"
                                  , className: "control-select"
                                  , value: climateAttribute
                                  , onChange: handler targetValue handleClimateAttrChange
                                  , children:
                                      [ R.option
                                          { value: "koeppen"
                                          , children: [ R.text "Köppen classification" ]
                                          }
                                      , R.option
                                          { value: "ocean_currents"
                                          , children: [ R.text "Ocean currents" ]
                                          }
                                      , R.option
                                          { value: "insolation"
                                          , children: [ R.text "Insolation" ]
                                          }
                                      , R.option
                                          { value: "precipitation"
                                          , children: [ R.text "Precipitation" ]
                                          }
                                      , R.option
                                          { value: "temperature"
                                          , children: [ R.text "Temperature" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Season:" ]
                                  }
                              , R.input
                                  { id: "season-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "100"
                                  , step: "1"
                                  , disabled: climateAttribute == "koeppen"
                                  , value: show climateSeason
                                  , onChange: handler targetValue handleClimateSeasonChange
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Glaciation:" ]
                                  }
                              , R.input
                                  { id: "glaciation-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "100"
                                  , step: "1"
                                  , value: show climateGlaciation
                                  , onChange: handler targetValue handleClimateGlaciationChange
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { id: "climate-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "climate-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text climateInfo.planetName ]
                                  }
                              , R.div
                                  { id: "climate-tiles-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show climateInfo.tileCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Circulation Cells" ]
                                  }
                              , R.select
                                  { id: "cells-select"
                                  , className: "control-select"
                                  , value: show climateCells
                                  , onChange: handler targetValue handleClimateCellsChange
                                  , children:
                                      [ R.option { value: "1", children: [ R.text "1 cell" ] }
                                      , R.option { value: "3", children: [ R.text "3 cells" ] }
                                      , R.option { value: "5", children: [ R.text "5 cells" ] }
                                      , R.option { value: "7", children: [ R.text "7 cells" ] }
                                      , R.option { value: "9", children: [ R.text "9 cells" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "climate-earth-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadEarthIntoClimate setClimateInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "climate-load-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadFilePlanetIntoClimate setClimateInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "climate-saved-btn"
                          , className: "action-btn"
                          , disabled: not climateInfo.hasSavedPlanet
                          , onClick: capture_ (loadSavedPlanetIntoClimate setClimateInfo)
                          , children: [ R.text "Load Saved" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      TectonicsScreen ->
        let
          continentWord = if tectonicsInfo.continentCount == 1 then " continent" else " continents"
          biosphereStatus =
            if tectonicsInfo.hasLife then "Atmosphere + Life"
            else if tectonicsInfo.hasAtmosphere then "Atmosphere active"
            else "Pre-atmosphere"
        in
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "tectonics-aspect-select"
                                  , className: "control-select"
                                  , value: tectonicsAspect
                                  , onChange: handler targetValue handleTectonicsAspectChange
                                  , children:
                                      [ R.option { value: "elevation", children: [ R.text "Elevation" ] }
                                      , R.option { value: "color", children: [ R.text "Color" ] }
                                      , R.option { value: "climate", children: [ R.text "Climate" ] }
                                      , R.option { value: "mountains", children: [ R.text "Mountains" ] }
                                      , R.option { value: "rock", children: [ R.text "Rock" ] }
                                      , R.option { value: "subduction", children: [ R.text "Subduction" ] }
                                      , R.option { value: "thickness", children: [ R.text "Thickness" ] }
                                      , R.option { value: "meteors", children: [ R.text "Meteors & Craters" ] }
                                      ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "tectonics-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (toggleTectonicsRunning setTectonicsInfo)
                                  , children: [ R.text (if tectonicsInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "tectonics-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: tectonicsInfo.isRunning
                                  , onClick: capture_ (stepTectonics setTectonicsInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "tectonics-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "tectonics-geoticks-label"
                                  , className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( show tectonicsInfo.geoticks
                                              <> " geoticks ("
                                              <> show (tectonicsInfo.geoticks * 5)
                                              <> " Myr)"
                                          )
                                      ]
                                  }
                              , R.div
                                  { id: "tectonics-continents-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show tectonicsInfo.continentCount <> continentWord) ]
                                  }
                              , R.div
                                  { id: "tectonics-land-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show tectonicsInfo.actualLandPercent <> "% land") ]
                                  }
                              , R.div
                                  { id: "tectonics-biosphere-label"
                                  , className: "stat-line stat-subtle"
                                  , children: [ R.text biosphereStatus ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Selection (Stratigraphy)" ]
                                  }
                              , R.pre
                                  { id: "tectonics-details"
                                  , className: "tile-details-box"
                                  , children: [ R.text tectonicsInfo.selectedTileReport ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Planet Size" ]
                                  }
                              , R.select
                                  { id: "tectonics-size-select"
                                  , className: "control-select"
                                  , disabled: tectonicsInfo.isRunning
                                  , value: show tectonicsSize
                                  , onChange: handler targetValue handleTectonicsSizeChange
                                  , children:
                                      [ R.option { value: "4", children: [ R.text "Very Small (2,100 km)" ] }
                                      , R.option { value: "5", children: [ R.text "Small (3,700 km)" ] }
                                      , R.option { value: "6", children: [ R.text "Medium / Earth (6,400 km)" ] }
                                      , R.option { value: "7", children: [ R.text "Large (11,000 km)" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text ("Initial Land (" <> show tectonicsLandPct <> "%)") ]
                                  }
                              , R.input
                                  { id: "tectonics-land-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "5"
                                  , max: "65"
                                  , step: "1"
                                  , disabled: tectonicsInfo.isRunning
                                  , value: show tectonicsLandPct
                                  , onChange: handler targetValue handleTectonicsLandPctChange
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "tectonics-save-btn"
                                  , className: "action-btn secondary"
                                  , disabled: tectonicsInfo.isRunning
                                  , onClick: capture_ (saveTectonicsPlanet setTectonicsInfo)
                                  , children: [ R.text "Save" ]
                                  }
                              , R.button
                                  { id: "tectonics-load-btn"
                                  , className: "action-btn secondary"
                                  , disabled: tectonicsInfo.isRunning
                                  , onClick: capture_ (loadTectonicsPlanetFromFile setTectonicsSize setTectonicsInfo)
                                  , children: [ R.text "Load" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "tectonics-reset-btn"
                                  , className: "action-btn secondary"
                                  , disabled: tectonicsInfo.isRunning
                                  , onClick: capture_ (resetTectonics tectonicsSize tectonicsLandPct setTectonicsInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      SplitScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Time:" ]
                                  }
                              , R.input
                                  { id: "split-time-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "2"
                                  , step: "1"
                                  , value: show splitTime
                                  , onChange: handler targetValue handleSplitTimeChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show splitTime) ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { id: "split-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "split-iterations-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Iterations: " <> show splitInfo.iterations) ]
                                  }
                              , R.div
                                  { id: "split-red-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Red half: " <> show splitInfo.redCount <> " tiles") ]
                                  }
                              , R.div
                                  { id: "split-blue-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Blue half: " <> show splitInfo.blueCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Planet Size" ]
                                  }
                              , R.select
                                  { id: "split-size-select"
                                  , className: "control-select"
                                  , value: show splitSize
                                  , onChange: handler targetValue handleSplitSizeChange
                                  , children:
                                      [ R.option { value: "4", children: [ R.text "Very Small (2,100 km)" ] }
                                      , R.option { value: "5", children: [ R.text "Small (3,700 km)" ] }
                                      , R.option { value: "6", children: [ R.text "Medium / Earth (6,400 km)" ] }
                                      , R.option { value: "7", children: [ R.text "Large (11,000 km)" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "split-iterate-btn"
                          , className: "action-btn"
                          , onClick: capture_ (iterateSplit 0 setSplitInfo)
                          , children: [ R.text "Iterate" ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "split-red-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (iterateSplit 1 setSplitInfo)
                                  , children: [ R.text "Split Red" ]
                                  }
                              , R.button
                                  { id: "split-blue-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (iterateSplit 2 setSplitInfo)
                                  , children: [ R.text "Split Blue" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "split-reset-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (resetSplit splitSize setSplitInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      MoveScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Direction:" ]
                                  }
                              , R.input
                                  { id: "move-direction-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show moveDirection
                                  , onChange: handler targetValue handleMoveDirectionChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show moveDirection <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Speed:" ]
                                  }
                              , R.input
                                  { id: "move-speed-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "15"
                                  , step: "1"
                                  , value: show moveSpeed
                                  , onChange: handler targetValue handleMoveSpeedChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show moveSpeed <> "°") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "move-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (toggleMoveRunning setMoveInfo)
                                  , children: [ R.text (if moveInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "move-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: moveInfo.isRunning
                                  , onClick: capture_ (stepMove setMoveInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "move-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "move-count-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Count: " <> show moveInfo.groupTileCount <> " tiles") ]
                                  }
                              , R.div
                                  { id: "move-steps-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Steps: " <> show moveInfo.steps) ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Planet Size" ]
                                  }
                              , R.select
                                  { id: "move-size-select"
                                  , className: "control-select"
                                  , disabled: moveInfo.isRunning
                                  , value: show moveSize
                                  , onChange: handler targetValue handleMoveSizeChange
                                  , children:
                                      [ R.option { value: "4", children: [ R.text "Very Small (2,100 km)" ] }
                                      , R.option { value: "5", children: [ R.text "Small (3,700 km)" ] }
                                      , R.option { value: "6", children: [ R.text "Medium / Earth (6,400 km)" ] }
                                      , R.option { value: "7", children: [ R.text "Large (11,000 km)" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "move-trail-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ handleToggleMoveTrail
                          , children: [ R.text (if moveTrail then "Trail: On" else "Trail: Off") ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "move-reset-btn"
                                  , className: "action-btn secondary"
                                  , disabled: moveInfo.isRunning
                                  , onClick: capture_ (resetMove moveSize (Int.toNumber moveDirection) (Int.toNumber moveSpeed) setMoveInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      ErosionScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.button
                          { id: "erosion-step-btn"
                          , className: "action-btn"
                          , onClick: capture_ (stepErosion setErosionInfo)
                          , children: [ R.text "Erode" ]
                          }
                      , R.div
                          { id: "erosion-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "erosion-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text erosionInfo.planetName ]
                                  }
                              , R.div
                                  { id: "erosion-steps-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Erosion steps: " <> show erosionInfo.steps) ]
                                  }
                              , R.div
                                  { id: "erosion-tiles-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show erosionInfo.tileCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "erosion-lost-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ handleToggleErosionLost
                                  , children: [ R.text (if erosionShowLost then "Lost: On" else "Lost: Off") ]
                                  }
                              , R.button
                                  { id: "erosion-gained-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ handleToggleErosionGained
                                  , children: [ R.text (if erosionShowGained then "Gained: On" else "Gained: Off") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Selection" ]
                                  }
                              , R.pre
                                  { id: "erosion-details"
                                  , className: "tile-details-box"
                                  , children: [ R.text erosionInfo.selectedTileReport ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "erosion-earth-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadEarthIntoErosion setErosionInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "erosion-load-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadFilePlanetIntoErosion setErosionInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "erosion-saved-btn"
                          , className: "action-btn secondary"
                          , disabled: not erosionInfo.hasSavedPlanet
                          , onClick: capture_ (loadSavedPlanetIntoErosion setErosionInfo)
                          , children: [ R.text "Load Saved" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      RiversScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Min peak height:" ]
                                  }
                              , R.input
                                  { id: "rivers-hmin-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "5"
                                  , max: "95"
                                  , step: "1"
                                  , value: show riversHmin
                                  , onChange: handler targetValue handleRiversHminChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show (Int.toNumber riversHmin / 10.0) <> " km") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Min source precip:" ]
                                  }
                              , R.input
                                  { id: "rivers-pmin-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "100"
                                  , step: "1"
                                  , value: show riversPmin
                                  , onChange: handler targetValue handleRiversPminChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show riversPmin <> "%") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { id: "rivers-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "rivers-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text riversInfo.planetName ]
                                  }
                              , R.div
                                  { id: "rivers-count-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Active rivers: " <> show riversInfo.riverCount) ]
                                  }
                              , R.div
                                  { id: "rivers-tiles-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show riversInfo.tileCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "rivers-earth-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadEarthIntoRivers setRiversInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "rivers-load-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadFilePlanetIntoRivers setRiversInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "rivers-saved-btn"
                          , className: "action-btn"
                          , disabled: not riversInfo.hasSavedPlanet
                          , onClick: capture_ (loadSavedPlanetIntoRivers setRiversInfo)
                          , children: [ R.text "Load Saved" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      LifeformsScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "lifeforms-attribute-select"
                                  , className: "control-select"
                                  , value: lifeformsAttribute
                                  , onChange: handler targetValue handleLifeformsAttrChange
                                  , children:
                                      [ R.option
                                          { value: "life"
                                          , children: [ R.text "Life" ]
                                          }
                                      , R.option
                                          { value: "animals"
                                          , children: [ R.text "Animals" ]
                                          }
                                      , R.option
                                          { value: "plants"
                                          , children: [ R.text "Plants" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Season:" ]
                                  }
                              , R.input
                                  { id: "lifeforms-season-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "100"
                                  , step: "1"
                                  , value: show lifeformsSeason
                                  , onChange: handler targetValue handleLifeformsSeasonChange
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Glaciation:" ]
                                  }
                              , R.input
                                  { id: "lifeforms-glaciation-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "100"
                                  , step: "1"
                                  , value: show lifeformsGlaciation
                                  , onChange: handler targetValue handleLifeformsGlaciationChange
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { id: "lifeforms-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "lifeforms-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text lifeformsInfo.planetName ]
                                  }
                              , R.div
                                  { id: "lifeforms-animals-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Settled animals: " <> show lifeformsInfo.settledAnimals) ]
                                  }
                              , R.div
                                  { id: "lifeforms-plants-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Settled plants: " <> show lifeformsInfo.settledPlants) ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children:
                                      [ R.text
                                          ( if lifeformsInfo.selectedTileIdx < 0 then
                                              "Species in Selected Tile (Click Globe)"
                                            else if lifeformsInfo.selectedTileRegion == "" then
                                              "Tile #" <> show lifeformsInfo.selectedTileIdx <> " — Click species to map range"
                                            else
                                              "Tile #" <> show lifeformsInfo.selectedTileIdx <> " (" <> lifeformsInfo.selectedTileRegion <> ") — Click species to map range"
                                          )
                                      ]
                                  }
                              , R.div
                                  { id: "lifeforms-species-list"
                                  , className: "species-list-box"
                                  , children:
                                      if lifeformsInfo.selectedTileIdx < 0 then
                                        [ R.div
                                            { className: "stat-subtle"
                                            , children: [ R.text "Click any land tile on the globe to list its animals and plants. Click a species in the list to highlight its global range in green." ]
                                            }
                                        ]
                                      else
                                        map
                                          ( \sp ->
                                              R.button
                                                { className:
                                                    if sp.id == lifeformsInfo.selectedSpeciesId then
                                                      "species-item-btn selected"
                                                    else
                                                      "species-item-btn"
                                                , onClick: capture_ (selectLifeformsSpecies sp.id setLifeformsInfo)
                                                , children: [ R.text ("[" <> sp.kind <> "] " <> sp.name) ]
                                                }
                                          )
                                          lifeformsInfo.tileSpecies
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "lifeforms-earth-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadEarthIntoLifeforms setLifeformsInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "lifeforms-load-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadFilePlanetIntoLifeforms setLifeformsInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "lifeforms-saved-btn"
                          , className: "action-btn"
                          , disabled: not lifeformsInfo.hasSavedPlanet
                          , onClick: capture_ (loadSavedPlanetIntoLifeforms setLifeformsInfo)
                          , children: [ R.text "Load Saved" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      PopulationScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Travel range:" ]
                                  }
                              , R.input
                                  { id: "population-range-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "12"
                                  , step: "1"
                                  , value: show populationTravelRange
                                  , onChange: handler targetValue handlePopulationTravelRangeChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show populationTravelRange) ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Coast prox:" ]
                                  }
                              , R.input
                                  { id: "population-coastprox-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "6"
                                  , step: "1"
                                  , value: show populationCoastProx
                                  , onChange: handler targetValue handlePopulationCoastProxChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show populationCoastProx) ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "population-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (togglePopulationRunning setPopulationInfo)
                                  , children: [ R.text (if populationInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "population-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: populationInfo.isRunning
                                  , onClick: capture_ (stepPopulation setPopulationInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "population-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "population-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text populationInfo.planetName ]
                                  }
                              , R.div
                                  { id: "population-ticks-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Steps: " <> show populationInfo.ticks) ]
                                  }
                              , R.div
                                  { id: "population-count-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Populated: " <> show populationInfo.populatedCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "population-earth-btn"
                                  , className: "action-btn secondary"
                                  , disabled: populationInfo.isRunning
                                  , onClick: capture_ (loadEarthIntoPopulation setPopulationInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "population-load-btn"
                                  , className: "action-btn secondary"
                                  , disabled: populationInfo.isRunning
                                  , onClick: capture_ (loadFilePlanetIntoPopulation setPopulationInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "population-saved-btn"
                          , className: "action-btn"
                          , disabled: populationInfo.isRunning || not populationInfo.hasSavedPlanet
                          , onClick: capture_ (loadSavedPlanetIntoPopulation setPopulationInfo)
                          , children: [ R.text "Load Saved" ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "population-reset-btn"
                                  , className: "action-btn secondary"
                                  , disabled: populationInfo.isRunning
                                  , onClick: capture_ (resetPopulation setPopulationInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      RacinationScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Travel range:" ]
                                  }
                              , R.input
                                  { id: "racination-range-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "12"
                                  , step: "1"
                                  , value: show racinationTravelRange
                                  , onChange: handler targetValue handleRacinationTravelRangeChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show racinationTravelRange) ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.button
                          { id: "racination-isolate-btn"
                          , className: "action-btn"
                          , onClick: capture_ (isolateRacination setRacinationInfo)
                          , children: [ R.text "Isolate" ]
                          }
                      , R.div
                          { id: "racination-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "racination-races-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Races: " <> show racinationInfo.peoples) ]
                                  }
                              , R.div
                                  { id: "racination-isolations-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Isolations: " <> show racinationInfo.isolations) ]
                                  }
                              , R.div
                                  { id: "racination-populated-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Populated: " <> show racinationInfo.populatedCount <> " tiles") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "racination-reset-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (resetRacination setRacinationInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      PrehistoryScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "prehistory-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (togglePrehistoryRunning setPrehistoryInfo)
                                  , children: [ R.text (if prehistoryInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "prehistory-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: prehistoryInfo.isRunning
                                  , onClick: capture_ (stepPrehistory setPrehistoryInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "prehistory-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "prehistory-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text prehistoryInfo.planetName ]
                                  }
                              , R.div
                                  { id: "prehistory-time-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show prehistoryInfo.ticks <> " ticks (" <> show prehistoryInfo.kyr <> " kyr)") ]
                                  }
                              , R.div
                                  { id: "prehistory-glaciation-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Glaciation: " <> show prehistoryInfo.glaciationPct <> "%") ]
                                  }
                              , R.div
                                  { id: "prehistory-races-label"
                                  , className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( show prehistoryInfo.races
                                              <> " races ("
                                              <> show prehistoryInfo.agRaces
                                              <> " agricultural)"
                                          )
                                      ]
                                  }
                              , R.div
                                  { id: "prehistory-population-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("Population: " <> prehistoryInfo.populationText) ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "prehistory-earth-btn"
                                  , className: "action-btn secondary"
                                  , disabled: prehistoryInfo.isRunning
                                  , onClick: capture_ (loadEarthIntoPrehistory setPrehistoryInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "prehistory-load-btn"
                                  , className: "action-btn secondary"
                                  , disabled: prehistoryInfo.isRunning
                                  , onClick: capture_ (loadFilePlanetIntoPrehistory setPrehistoryInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "prehistory-saved-btn"
                                  , className: "action-btn"
                                  , disabled: prehistoryInfo.isRunning || not prehistoryInfo.hasSavedPlanet
                                  , onClick: capture_ (loadSavedPlanetIntoPrehistory setPrehistoryInfo)
                                  , children: [ R.text "Load Saved" ]
                                  }
                              , R.button
                                  { id: "prehistory-save-btn"
                                  , className: "action-btn"
                                  , disabled: prehistoryInfo.isRunning
                                  , onClick: capture_ (savePrehistoryPlanet setPrehistoryInfo)
                                  , children: [ R.text "Save" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "prehistory-reset-btn"
                                  , className: "action-btn secondary"
                                  , disabled: prehistoryInfo.isRunning
                                  , onClick: capture_ (resetPrehistory setPrehistoryInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      LanguageScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "language-columns"
                  , children:
                      [ R.div
                          { className: "language-col"
                          , children:
                              [ R.div
                                  { className: "control-label"
                                  , children: [ R.text "Source Phonemes" ]
                                  }
                              , R.div
                                  { id: "language-source-phonemes"
                                  , className: "language-phonemes-box"
                                  , children: [ R.text languageInfo.sourcePhonemes ]
                                  }
                              , R.div
                                  { className: "control-label"
                                  , children: [ R.text "Source Lexicon" ]
                                  }
                              , R.div
                                  { id: "language-source-words"
                                  , className: "language-word-list"
                                  , children:
                                      map
                                        ( \w ->
                                            R.div
                                              { className: "language-word-row"
                                              , children:
                                                  [ R.div
                                                      { className: "language-word-main"
                                                      , children:
                                                          [ R.span
                                                              { className: "language-word-etym"
                                                              , children: [ R.text (w.concept <> ": ") ]
                                                              }
                                                          , R.span_ [ R.text w.sourceOrth ]
                                                          , R.span
                                                              { className: "language-word-ipa"
                                                              , children: [ R.text ("[" <> w.sourceIpa <> "]") ]
                                                              }
                                                          ]
                                                      }
                                                  ]
                                              }
                                        )
                                        languageInfo.words
                                  }
                              ]
                          }
                      , R.div
                          { className: "language-col"
                          , children:
                              [ R.div
                                  { className: "control-label"
                                  , children: [ R.text "Destination Phonemes" ]
                                  }
                              , R.div
                                  { id: "language-dest-phonemes"
                                  , className: "language-phonemes-box"
                                  , children: [ R.text languageInfo.destPhonemes ]
                                  }
                              , R.div
                                  { className: "control-label"
                                  , children: [ R.text "Mutated / Borrowed Lexicon" ]
                                  }
                              , R.div
                                  { id: "language-dest-words"
                                  , className: "language-word-list"
                                  , children:
                                      map
                                        ( \w ->
                                            R.div
                                              { className: "language-word-row"
                                              , children:
                                                  [ R.div
                                                      { className: "language-word-main"
                                                      , children:
                                                          [ R.span
                                                              { className: "language-word-etym"
                                                              , children: [ R.text (w.concept <> ": ") ]
                                                              }
                                                          , R.span_ [ R.text w.destOrth ]
                                                          , R.span
                                                              { className: "language-word-ipa"
                                                              , children: [ R.text ("[" <> w.destIpa <> "]") ]
                                                              }
                                                          ]
                                                      }
                                                  , R.div
                                                      { className: "language-word-etym"
                                                      , children: [ R.text w.etymology ]
                                                      }
                                                  ]
                                              }
                                        )
                                        languageInfo.words
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column pangea-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "language-generate-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (generateLanguage setLanguageInfo)
                                  , children: [ R.text "Generate" ]
                                  }
                              , R.button
                                  { id: "language-borrow-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (borrowLanguage setLanguageInfo)
                                  , children: [ R.text "Borrow" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-label"
                          , children: [ R.text "Vowel Mutations" ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "language-amutate-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (applyLanguageSoundChange 0 setLanguageInfo)
                                  , children: [ R.text "A-Mutate" ]
                                  }
                              , R.button
                                  { id: "language-imutate-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (applyLanguageSoundChange 1 setLanguageInfo)
                                  , children: [ R.text "I-Mutate" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "language-break-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (applyLanguageSoundChange 2 setLanguageInfo)
                          , children: [ R.text "Break Vowels" ]
                          }
                      , R.div
                          { className: "control-label"
                          , children: [ R.text "Consonant Lenition" ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "language-open-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (applyLanguageSoundChange 3 setLanguageInfo)
                                  , children: [ R.text "Open" ]
                                  }
                              , R.button
                                  { id: "language-sonorize-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (applyLanguageSoundChange 4 setLanguageInfo)
                                  , children: [ R.text "Sonorize" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "language-vocalize-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (applyLanguageSoundChange 5 setLanguageInfo)
                          , children: [ R.text "Vocalize" ]
                          }
                      , R.div
                          { className: "control-label"
                          , children: [ R.text "Vowel Elision" ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "language-apheresis-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (applyLanguageSoundChange 6 setLanguageInfo)
                                  , children: [ R.text "Apheresis" ]
                                  }
                              , R.button
                                  { id: "language-syncope-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (applyLanguageSoundChange 7 setLanguageInfo)
                                  , children: [ R.text "Syncope" ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "language-apocope-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (applyLanguageSoundChange 8 setLanguageInfo)
                          , children: [ R.text "Apocope" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      TerrainScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.button
                                  { id: "terrain-rivers-btn"
                                  , className: if terrainShowRivers then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleTerrainRivers
                                  , children: [ R.text (if terrainShowRivers then "Rivers: On" else "Rivers: Off") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Subdivided Terrain & River Routing Inspector" ]
                                  }
                              , R.div
                                  { id: "terrain-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text terrainInfo.selectedReport ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { id: "terrain-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "terrain-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text terrainInfo.planetName ]
                                  }
                              , R.div
                                  { id: "terrain-fine-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show terrainInfo.fineFaceCount <> " fine land tiles") ]
                                  }
                              , R.div
                                  { id: "terrain-display-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show terrainInfo.displayFaceCount <> " total display faces") ]
                                  }
                              , R.div
                                  { id: "terrain-coarse-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show terrainInfo.coarseTileCount <> " coarse parent tiles") ]
                                  }
                              , R.div
                                  { id: "terrain-rivers-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show terrainInfo.riverCount <> " fine-routed rivers") ]
                                  }
                              ]
                          }
                      , R.button
                          { id: "terrain-earth-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (loadEarthIntoTerrain setTerrainInfo)
                          , children: [ R.text "Use Earth (ETOPO2)" ]
                          }
                      , R.button
                          { id: "terrain-saved-btn"
                          , className: "action-btn secondary"
                          , disabled: not terrainInfo.hasSavedPlanet
                          , onClick: capture_ (loadSavedPlanetIntoTerrain setTerrainInfo)
                          , children: [ R.text "Use Saved Planet" ]
                          }
                      , R.button
                          { id: "terrain-load-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (loadFilePlanetIntoTerrain setTerrainInfo)
                          , children: [ R.text "Load (.tec11)" ]
                          }
                      , R.button
                          { id: "terrain-save-btn"
                          , className: "action-btn"
                          , onClick: capture_ (saveTerrainPlanet setTerrainInfo)
                          , children: [ R.text "Save (.tec11)" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ (setScreen MainMenu)
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }

      TravelScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "travel-aspect-select"
                                  , className: "control-select"
                                  , value: travelAspect
                                  , onChange: handler targetValue handleTravelAspectChange
                                  , children:
                                      [ R.option
                                          { value: "travel"
                                          , children: [ R.text "Travel Routes & Ports" ]
                                          }
                                      , R.option
                                          { value: "winds_currents"
                                          , children: [ R.text "Winds & Ocean Currents" ]
                                          }
                                      , R.option
                                          { value: "nations"
                                          , children: [ R.text "Nations & Maritime Diplomacy" ]
                                          }
                                      , R.option
                                          { value: "population"
                                          , children: [ R.text "Population & Migration" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "travel-winds-btn"
                                  , className: if travelShowWindsCurrents then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleTravelWindsCurrents
                                  , children: [ R.text (if travelShowWindsCurrents then "Winds & Currents: On" else "Winds & Currents: Off") ]
                                  }
                              , R.button
                                  { id: "travel-potential-btn"
                                  , className: if travelShowPotential then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleTravelPotential
                                  , children: [ R.text (if travelShowPotential then "All Corridors: On" else "Active Only") ]
                                  }
                              , R.button
                                  { id: "travel-rivers-btn"
                                  , className: if travelShowRivers then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleTravelRivers
                                  , children: [ R.text (if travelShowRivers then "Rivers: On" else "Rivers: Off") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "travel-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (toggleTravelRunning setTravelInfo)
                                  , children: [ R.text (if travelInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "travel-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: travelInfo.isRunning
                                  , onClick: capture_ (stepTravel setTravelInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "travel-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "travel-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text (travelInfo.planetName <> " — " <> show travelInfo.ticks <> " ticks (" <> show travelInfo.years <> " yrs)") ]
                                  }
                              , R.div
                                  { id: "travel-routes-label"
                                  , className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( "Active Routes: "
                                              <> show travelInfo.activeRoutes
                                              <> " / "
                                              <> show travelInfo.potentialRoutes
                                              <> " corridors ("
                                              <> show travelInfo.portCount
                                              <> " ports)"
                                          )
                                      ]
                                  }
                              , R.div
                                  { id: "travel-diplomacy-label"
                                  , className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( "Maritime Trade: "
                                              <> show travelInfo.tradeRoutes
                                              <> " | Maritime Conflict: "
                                              <> show travelInfo.conflictRoutes
                                              <> " | States: "
                                              <> show travelInfo.activeStates
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Travel Routes, Winds, Ocean Currents & Port Inspection (click globe)" ]
                                  }
                              , R.div
                                  { id: "travel-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text travelInfo.selectedReport ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "travel-earth-btn"
                                  , className: "action-btn secondary"
                                  , disabled: travelInfo.isRunning
                                  , onClick: capture_ (loadEarthIntoTravel setTravelInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "travel-load-btn"
                                  , className: "action-btn secondary"
                                  , disabled: travelInfo.isRunning
                                  , onClick: capture_ (loadFilePlanetIntoTravel setTravelInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "travel-saved-btn"
                                  , className: "action-btn"
                                  , disabled: travelInfo.isRunning || not travelInfo.hasSavedPlanet
                                  , onClick: capture_ (loadSavedPlanetIntoTravel setTravelInfo)
                                  , children: [ R.text "Load Saved" ]
                                  }
                              , R.button
                                  { id: "travel-save-btn"
                                  , className: "action-btn"
                                  , disabled: travelInfo.isRunning
                                  , onClick: capture_ (saveTravelPlanet setTravelInfo)
                                  , children: [ R.text "Save" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "travel-reset-btn"
                                  , className: "action-btn secondary"
                                  , disabled: travelInfo.isRunning
                                  , onClick: capture_ (resetTravel setTravelInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      HistoryScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "history-aspect-select"
                                  , className: "control-select"
                                  , value: historyAspect
                                  , onChange: handler targetValue handleHistoryAspectChange
                                  , children:
                                      [ R.option
                                          { value: "nations"
                                          , children: [ R.text "Nations & Diplomacy" ]
                                          }
                                      , R.option
                                          { value: "travel"
                                          , children: [ R.text "Travel Routes & Sea Lanes" ]
                                          }
                                      , R.option
                                          { value: "winds_currents"
                                          , children: [ R.text "Winds & Ocean Currents" ]
                                          }
                                      , R.option
                                          { value: "species"
                                          , children: [ R.text "Domesticable Species" ]
                                          }
                                      , R.option
                                          { value: "population"
                                          , children: [ R.text "Population" ]
                                          }
                                      , R.option
                                          { value: "capacity"
                                          , children: [ R.text "Carrying Capacity" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "history-rivers-btn"
                                  , className: if historyShowRivers then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleHistoryRivers
                                  , children: [ R.text (if historyShowRivers then "Rivers: Shown" else "Rivers: Hidden") ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "history-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (toggleHistoryRunning setHistoryInfo)
                                  , children: [ R.text (if historyInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "history-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: historyInfo.isRunning
                                  , onClick: capture_ (stepHistory setHistoryInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "history-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "history-planet-label"
                                  , className: "stat-line"
                                  , children: [ R.text historyInfo.planetName ]
                                  }
                              , R.div
                                  { id: "history-time-label"
                                  , className: "stat-line"
                                  , children: [ R.text (show historyInfo.ticks <> " ticks (" <> show historyInfo.years <> " yrs)") ]
                                  }
                              , R.div
                                  { id: "history-states-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("States: " <> show historyInfo.activeStates <> " | Pop: " <> historyInfo.populationText) ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "State, Diplomacy & Lexicon (click globe)" ]
                                  }
                              , R.div
                                  { id: "history-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text historyInfo.selectedReport ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "history-earth-btn"
                                  , className: "action-btn secondary"
                                  , disabled: historyInfo.isRunning
                                  , onClick: capture_ (loadEarthIntoHistory setHistoryInfo)
                                  , children: [ R.text "Earth" ]
                                  }
                              , R.button
                                  { id: "history-load-btn"
                                  , className: "action-btn secondary"
                                  , disabled: historyInfo.isRunning
                                  , onClick: capture_ (loadFilePlanetIntoHistory setHistoryInfo)
                                  , children: [ R.text "Load File" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "history-saved-btn"
                                  , className: "action-btn"
                                  , disabled: historyInfo.isRunning || not historyInfo.hasSavedPlanet
                                  , onClick: capture_ (loadSavedPlanetIntoHistory setHistoryInfo)
                                  , children: [ R.text "Load Saved" ]
                                  }
                              , R.button
                                  { id: "history-save-btn"
                                  , className: "action-btn"
                                  , disabled: historyInfo.isRunning
                                  , onClick: capture_ (saveHistoryPlanet setHistoryInfo)
                                  , children: [ R.text "Save" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "history-reset-btn"
                                  , className: "action-btn secondary"
                                  , disabled: historyInfo.isRunning
                                  , onClick: capture_ (resetHistory setHistoryInfo)
                                  , children: [ R.text "Reset" ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      WorldScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "world-aspect-select"
                                  , className: "control-select"
                                  , value: worldAspect
                                  , onChange: handler targetValue handleWorldAspectChange
                                  , children:
                                      [ R.option
                                          { value: "color"
                                          , children: [ R.text "Color" ]
                                          }
                                      , R.option
                                          { value: "climate"
                                          , children: [ R.text "Climate" ]
                                          }
                                      , R.option
                                          { value: "population"
                                          , children: [ R.text "Population" ]
                                          }
                                      , R.option
                                          { value: "nations"
                                          , children: [ R.text "Nations" ]
                                          }
                                      , R.option
                                          { value: "travel"
                                          , children: [ R.text "Travel Routes" ]
                                          }
                                      , R.option
                                          { value: "meteors"
                                          , children: [ R.text "Meteors & Craters" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Tile Details (Layers, Climate, Population & Ancestry)" ]
                                  }
                              , R.div
                                  { id: "world-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text worldInfo.selectedReport ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Planet Size" ]
                                  }
                              , R.select
                                  { id: "world-size-select"
                                  , className: "control-select"
                                  , disabled: worldInfo.isRunning
                                  , value: show worldSize
                                  , onChange: handler targetValue handleWorldSizeChange
                                  , children:
                                      [ R.option
                                          { value: "4"
                                          , children: [ R.text "Very Small (2,100 km — 812 tiles)" ]
                                          }
                                      , R.option
                                          { value: "5"
                                          , children: [ R.text "Small (3,700 km — 2,432 tiles)" ]
                                          }
                                      , R.option
                                          { value: "6"
                                          , children: [ R.text "Medium / Earth (6,400 km — 7,292 tiles)" ]
                                          }
                                      , R.option
                                          { value: "7"
                                          , children: [ R.text "Large (11,000 km — 21,872 tiles)" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "world-randomize-btn"
                                  , className: if worldRandomize then "action-btn" else "action-btn secondary"
                                  , disabled: worldInfo.isRunning
                                  , onClick: capture_ handleToggleWorldRandomize
                                  , children: [ R.text (if worldRandomize then "Randomize: On" else "Randomize: Off") ]
                                  }
                              , R.button
                                  { id: "world-create-btn"
                                  , className: "action-btn"
                                  , disabled: worldInfo.isRunning
                                  , onClick: capture_ handleCreateWorld
                                  , children: [ R.text "Create" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Day:" ]
                                  }
                              , R.input
                                  { id: "world-spin-select"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "6"
                                  , max: "48"
                                  , step: "1"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldDayHours
                                  , onChange: handler targetValue handleWorldDayHoursChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldDayHours <> "h") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Tilt:" ]
                                  }
                              , R.input
                                  { id: "world-tilt-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "45"
                                  , step: "1"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldTiltDeg
                                  , onChange: handler targetValue handleWorldTiltChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldTiltDeg <> "°") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Land %:" ]
                                  }
                              , R.input
                                  { id: "world-land-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "50"
                                  , step: "1"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldLandPct
                                  , onChange: handler targetValue handleWorldLandChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldLandPct <> "%") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Atm Δt:" ]
                                  }
                              , R.input
                                  { id: "world-atmdt-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "5"
                                  , max: "500"
                                  , step: "5"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldAtmDt
                                  , onChange: handler targetValue handleWorldAtmDtChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldAtmDt <> "M") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Life Δt:" ]
                                  }
                              , R.input
                                  { id: "world-lifedt-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "5"
                                  , max: "500"
                                  , step: "5"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldLifeDt
                                  , onChange: handler targetValue handleWorldLifeDtChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldLifeDt <> "M") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "People Δt:" ]
                                  }
                              , R.input
                                  { id: "world-peopledt-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "5"
                                  , max: "500"
                                  , step: "5"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldPeopleDt
                                  , onChange: handler targetValue handleWorldPeopleDtChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldPeopleDt <> "M") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Moons:" ]
                                  }
                              , R.select
                                  { id: "world-moons-select"
                                  , className: "control-select"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldMoonCount
                                  , onChange: handler targetValue handleWorldMoonCountChange
                                  , children:
                                      [ R.option { value: "0", children: [ R.text "0 (None)" ] }
                                      , R.option { value: "1", children: [ R.text "1 (Earth-like)" ] }
                                      , R.option { value: "2", children: [ R.text "2 (2:1 Harmonic)" ] }
                                      , R.option { value: "3", children: [ R.text "3 (4:2:1 Laplace)" ] }
                                      , R.option { value: "4", children: [ R.text "4 (8:4:2:1 Chain)" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Moon Dist:" ]
                                  }
                              , R.input
                                  { id: "world-moon-dist-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "25"
                                  , max: "95"
                                  , step: "5"
                                  , disabled: worldRandomize || worldInfo.isRunning || worldMoonCount == 0
                                  , value: show worldMoonDistRadii
                                  , onChange: handler targetValue handleWorldMoonDistChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show worldMoonDistRadii <> " Rp") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Moon Mass:" ]
                                  }
                              , R.input
                                  { id: "world-moon-mass-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "2"
                                  , max: "25"
                                  , step: "1"
                                  , disabled: worldRandomize || worldInfo.isRunning || worldMoonCount == 0
                                  , value: show worldMoonMassTenths
                                  , onChange: handler targetValue handleWorldMoonMassChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children:
                                      [ R.text
                                          ( if worldMoonCount == 0 then
                                              "0.0x"
                                            else
                                              show (worldMoonMassTenths / 10) <> "." <> show (worldMoonMassTenths `mod` 10) <> "x"
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Meteors:" ]
                                  }
                              , R.input
                                  { id: "world-meteor-freq-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "50"
                                  , step: "2"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldMeteorFreqTenths
                                  , onChange: handler targetValue handleWorldMeteorFreqChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children:
                                      [ R.text
                                          ( if worldMeteorFreqTenths == 0 then
                                              "0.0x (Off)"
                                            else
                                              show (worldMeteorFreqTenths / 10) <> "." <> show (worldMeteorFreqTenths `mod` 10) <> "x"
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Sun:" ]
                                  }
                              , R.select
                                  { id: "world-sun-select"
                                  , className: "control-select"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldSunFilter
                                  , onChange: handler targetValue handleWorldSunFilterChange
                                  , children:
                                      [ R.option { value: "2", children: [ R.text "G-Class Yellow (Sol)" ] }
                                      , R.option { value: "3", children: [ R.text "K-Class Orange" ] }
                                      , R.option { value: "1", children: [ R.text "F-Class White-Yellow" ] }
                                      , R.option { value: "4", children: [ R.text "M-Class Red Dwarf" ] }
                                      , R.option { value: "5", children: [ R.text "A-Class Blue-White" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Gas:" ]
                                  }
                              , R.select
                                  { id: "world-atmosphere-select"
                                  , className: "control-select"
                                  , disabled: worldRandomize || worldInfo.isRunning
                                  , value: show worldAtmosphereId
                                  , onChange: handler targetValue handleWorldAtmosphereChange
                                  , children:
                                      [ R.option { value: "0", children: [ R.text "N2-O2 + O3 (Earth)" ] }
                                      , R.option { value: "1", children: [ R.text "N2-CH4 (Methane)" ] }
                                      , R.option { value: "2", children: [ R.text "Dense CO2-N2 (4.5 bar)" ] }
                                      , R.option { value: "3", children: [ R.text "Tholin Haze (Titan)" ] }
                                      , R.option { value: "4", children: [ R.text "Dusty CO2 (Mars)" ] }
                                      , R.option { value: "5", children: [ R.text "H2-He-N2 (Hycean)" ] }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "world-start-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (toggleWorldRunning setWorldInfo)
                                  , children: [ R.text (if worldInfo.isRunning then "Pause" else "Start") ]
                                  }
                              , R.button
                                  { id: "world-step-btn"
                                  , className: "action-btn secondary"
                                  , disabled: worldInfo.isRunning
                                  , onClick: capture_ (stepWorld setWorldInfo)
                                  , children: [ R.text "Step" ]
                                  }
                              , R.button
                                  { id: "world-next-stage-btn"
                                  , className: "action-btn secondary"
                                  , disabled: worldInfo.isRunning || worldInfo.stageIndex >= 2
                                  , onClick: capture_ (advanceWorldStage setWorldInfo)
                                  , children: [ R.text "Next Stage" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "world-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { id: "world-stage-label"
                                  , className: "stat-line"
                                  , children: [ R.text worldInfo.stageName ]
                                  }
                              , R.div
                                  { id: "world-orbital-label"
                                  , className: "stat-line"
                                  , children: [ R.text worldInfo.orbitalSummaryText ]
                                  }
                              , R.div
                                  { id: "world-optics-label"
                                  , className: "stat-line"
                                  , style: R.css { display: "flex", alignItems: "center", gap: "6px", flexWrap: "wrap" }
                                  , children:
                                      [ R.span
                                          { style: R.css { display: "inline-block", width: "11px", height: "11px", borderRadius: "50%", backgroundColor: worldInfo.sunColorCss, boxShadow: "0 0 5px " <> worldInfo.sunColorCss }
                                          , children: []
                                          }
                                      , R.span { children: [ R.text (worldInfo.sunSpectralType <> " |") ] }
                                      , R.span
                                          { style: R.css { display: "inline-block", width: "11px", height: "11px", borderRadius: "50%", backgroundColor: worldInfo.skyColorCss }
                                          , children: []
                                          }
                                      , R.span { children: [ R.text ("Sky: " <> worldInfo.skyColorName <> " |") ] }
                                      , R.span
                                          { style: R.css { display: "inline-block", width: "11px", height: "11px", borderRadius: "50%", backgroundColor: worldInfo.sunsetColorCss }
                                          , children: []
                                          }
                                      , R.span { children: [ R.text ("Sunset: " <> worldInfo.sunsetColorName <> " |") ] }
                                      , R.span
                                          { style: R.css { display: "inline-block", width: "11px", height: "11px", borderRadius: "50%", backgroundColor: worldInfo.plantColorCss }
                                          , children: []
                                          }
                                      , R.span { children: [ R.text ("Flora: " <> worldInfo.plantColorName) ] }
                                      ]
                                  }
                              , R.div
                                  { id: "world-planets-label"
                                  , className: "stat-line"
                                  , children: [ R.text ("System: " <> worldInfo.planetsSummaryText) ]
                                  }
                              , R.div
                                  { id: "world-years-label"
                                  , className: "stat-line"
                                  , children: [ R.text (worldInfo.yearsText <> " years") ]
                                  }
                              , R.div
                                  { id: "world-population-label"
                                  , className: "stat-line"
                                  , children: [ R.text (worldInfo.populationText <> " people") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "world-load-btn"
                                  , className: "action-btn secondary"
                                  , disabled: worldInfo.isRunning
                                  , onClick: capture_ (loadFilePlanetIntoWorld setWorldInfo)
                                  , children: [ R.text "Load..." ]
                                  }
                              , R.button
                                  { id: "world-saved-btn"
                                  , className: "action-btn secondary"
                                  , disabled: worldInfo.isRunning || not worldInfo.hasSavedPlanet
                                  , onClick: capture_ (loadSavedPlanetIntoWorld setWorldInfo)
                                  , children: [ R.text "Load Saved" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "world-save-btn"
                                  , className: "action-btn"
                                  , disabled: worldInfo.isRunning
                                  , onClick: capture_ (saveWorldPlanet setWorldInfo)
                                  , children: [ R.text "Save..." ]
                                  }
                              , R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      MoonScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "moon-aspect-select"
                                  , className: "control-select"
                                  , value: moonAspect
                                  , onChange: handler targetValue handleMoonAspectChange
                                  , children:
                                      [ R.option
                                          { value: "tidal_erosion"
                                          , children: [ R.text "Tidal Scour & Shelf Deposition" ]
                                          }
                                      , R.option
                                          { value: "tidal_range"
                                          , children: [ R.text "Tidal Range (m) & Bulge" ]
                                          }
                                      , R.option
                                          { value: "climate"
                                          , children: [ R.text "Climate (Spin & Tilt)" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Tile Inspection (Local Tidal Range, Coastal Scour vs. Rain Erosion & Strata)" ]
                                  }
                              , R.div
                                  { id: "moon-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text moonInfo.selectedReport ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Harmonic Moons (2:1 Laplace Chain)" ]
                                  }
                              , R.select
                                  { id: "moon-count-select"
                                  , className: "control-select"
                                  , value: show moonCount
                                  , onChange: handler targetValue handleMoonCountChange
                                  , children:
                                      [ R.option
                                          { value: "0"
                                          , children: [ R.text "0 Moons (Solar Tides Only)" ]
                                          }
                                      , R.option
                                          { value: "1"
                                          , children: [ R.text "1 Moon (Earth-like)" ]
                                          }
                                      , R.option
                                          { value: "2"
                                          , children: [ R.text "2 Moons (2:1 Harmonic Resonance)" ]
                                          }
                                      , R.option
                                          { value: "3"
                                          , children: [ R.text "3 Moons (4:2:1 Laplace Chain)" ]
                                          }
                                      , R.option
                                          { value: "4"
                                          , children: [ R.text "4 Moons (8:4:2:1 Laplace Chain)" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Primary Mass:" ]
                                  }
                              , R.input
                                  { id: "moon-mass-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "30"
                                  , step: "1"
                                  , disabled: moonCount == 0
                                  , value: show moonMassTenths
                                  , onChange: handler targetValue handleMoonMassChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children:
                                      [ R.text
                                          ( if moonCount == 0 then
                                              "0.0x"
                                            else
                                              show (moonMassTenths / 10) <> "." <> show (moonMassTenths `mod` 10) <> "x"
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Target Dist:" ]
                                  }
                              , R.input
                                  { id: "moon-dist-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "20"
                                  , max: "100"
                                  , step: "2"
                                  , disabled: moonCount == 0
                                  , value: show moonDistRadii
                                  , onChange: handler targetValue handleMoonDistChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show moonDistRadii <> " Rp") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Epoch (Recession):" ]
                                  }
                              , R.input
                                  { id: "moon-epoch-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "0"
                                  , max: "100"
                                  , step: "5"
                                  , value: show moonEpochPct
                                  , onChange: handler targetValue handleMoonEpochChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children:
                                      [ R.text
                                          ( if moonEpochPct == 0 then
                                              "0% (Hadean)"
                                            else if moonEpochPct == 100 then
                                              "100% (Mature)"
                                            else
                                              show moonEpochPct <> "%"
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "moon-toggle-scour-btn"
                                  , className: if moonShowTidalScour then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleMoonTidalScour
                                  , children: [ R.text (if moonShowTidalScour then "Tidal Scour (Gold): On" else "Tidal Scour: Off") ]
                                  }
                              , R.button
                                  { id: "moon-toggle-dep-btn"
                                  , className: if moonShowDeposition then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleMoonDeposition
                                  , children: [ R.text (if moonShowDeposition then "Shelf Dep (Cyan): On" else "Shelf Dep: Off") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "moon-toggle-fluvial-btn"
                                  , className: if moonShowFluvial then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleMoonFluvial
                                  , children: [ R.text (if moonShowFluvial then "Rain Erosion (Red): On" else "Rain Erosion: Off") ]
                                  }
                              , R.button
                                  { id: "moon-erode-step-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (stepMoonErode setMoonInfo)
                                  , children: [ R.text "Erode Step" ]
                                  }
                              , R.button
                                  { id: "moon-reset-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (resetMoonLayers setMoonInfo)
                                  , children: [ R.text "Reset Strata" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "moon-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { className: "stat-line"
                                  , children: [ R.text ("Planet: " <> moonInfo.planetName <> " (Step " <> show moonInfo.steps <> ")") ]
                                  }
                              , R.div
                                  { id: "moon-harmonic-summary"
                                  , className: "stat-line"
                                  , children: [ R.text moonInfo.harmonicSummary ]
                                  }
                              , R.div
                                  { className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( "Coastal Tidal Scour: "
                                              <> moonInfo.meanCoastalScourText
                                              <> " ("
                                              <> moonInfo.tidalSharePctText
                                              <> " of coastal erosion) | Max Range: "
                                              <> moonInfo.maxTidalRangeText
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "moon-load-earth-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadEarthIntoMoon setMoonInfo)
                                  , children: [ R.text "Load Earth" ]
                                  }
                              , R.button
                                  { id: "moon-load-saved-btn"
                                  , className: "action-btn secondary"
                                  , disabled: not moonInfo.hasSavedPlanet
                                  , onClick: capture_ (loadSavedPlanetIntoMoon setMoonInfo)
                                  , children: [ R.text "Load Saved" ]
                                  }
                              , R.button
                                  { id: "moon-load-file-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadFilePlanetIntoMoon setMoonInfo)
                                  , children: [ R.text "Load..." ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      MeteorScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "meteor-aspect-select"
                                  , className: "control-select"
                                  , value: meteorAspect
                                  , onChange: handler targetValue handleMeteorAspectChange
                                  , children:
                                      [ R.option
                                          { value: "crater_ejecta"
                                          , children: [ R.text "Crater Basin, Ejecta Rim & Blast Wave" ]
                                          }
                                      , R.option
                                          { value: "topography"
                                          , children: [ R.text "Topography & Flooded Craters" ]
                                          }
                                      , R.option
                                          { value: "impact_winter"
                                          , children: [ R.text "Impact Winter & Biosphere Survival" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Tile Inspection (Crater Excavation, Ejecta Breccia, Impact Winter & Strata)" ]
                                  }
                              , R.div
                                  { id: "meteor-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text meteorInfo.selectedReport ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Impactor Composition & Density" ]
                                  }
                              , R.select
                                  { id: "meteor-density-select"
                                  , className: "control-select"
                                  , value: show meteorDensityKgM3
                                  , onChange: handler targetValue handleMeteorDensityChange
                                  , children:
                                      [ R.option
                                          { value: "1000"
                                          , children: [ R.text "Icy Comet (1,000 kg/m³)" ]
                                          }
                                      , R.option
                                          { value: "3000"
                                          , children: [ R.text "Stony Chondrite Asteroid (3,000 kg/m³)" ]
                                          }
                                      , R.option
                                          { value: "7800"
                                          , children: [ R.text "Metallic Iron-Nickel Core (7,800 kg/m³)" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Diameter:" ]
                                  }
                              , R.input
                                  { id: "meteor-diameter-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "1"
                                  , max: "100"
                                  , step: "1"
                                  , value: show meteorDiameterKm
                                  , onChange: handler targetValue handleMeteorDiameterChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show meteorDiameterKm <> " km") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.label
                                  { className: "slider-inline-label"
                                  , children: [ R.text "Velocity:" ]
                                  }
                              , R.input
                                  { id: "meteor-velocity-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "11"
                                  , max: "72"
                                  , step: "1"
                                  , value: show meteorVelocityKmS
                                  , onChange: handler targetValue handleMeteorVelocityChange
                                  }
                              , R.span
                                  { className: "rotation-label"
                                  , children: [ R.text (show meteorVelocityKmS <> " km/s") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "meteor-strike-target-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (strikeMeteorTile false setMeteorInfo)
                                  , children: [ R.text "Strike Selected / Random" ]
                                  }
                              , R.button
                                  { id: "meteor-strike-random-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (strikeMeteorTile true setMeteorInfo)
                                  , children: [ R.text "Strike Random" ]
                                  }
                              , R.button
                                  { id: "meteor-barrage-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (strikeMeteorBarrage setMeteorInfo)
                                  , children: [ R.text "LHB Barrage (8x)" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "meteor-toggle-ejecta-btn"
                                  , className: if meteorShowEjecta then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleMeteorEjecta
                                  , children: [ R.text (if meteorShowEjecta then "Ejecta Rim (Gold): On" else "Ejecta Rim: Off") ]
                                  }
                              , R.button
                                  { id: "meteor-toggle-blast-btn"
                                  , className: if meteorShowBlast then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleMeteorBlast
                                  , children: [ R.text (if meteorShowBlast then "Blast Wave (Violet): On" else "Blast Wave: Off") ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "meteor-recover-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (stepMeteorRecover setMeteorInfo)
                                  , children: [ R.text "Erode & Recover Step" ]
                                  }
                              , R.button
                                  { id: "meteor-reset-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (resetMeteorPlanet setMeteorInfo)
                                  , children: [ R.text "Reset Planet" ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "meteor-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( "Planet: "
                                              <> meteorInfo.planetName
                                              <> " | Strikes: "
                                              <> show meteorInfo.impacts
                                              <> " | Recovery Steps: "
                                              <> show meteorInfo.recoverySteps
                                          )
                                      ]
                                  }
                              , R.div
                                  { id: "meteor-impact-summary"
                                  , className: "stat-line"
                                  , children: [ R.text meteorInfo.impactSummary ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "meteor-load-earth-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadEarthIntoMeteor setMeteorInfo)
                                  , children: [ R.text "Load Earth" ]
                                  }
                              , R.button
                                  { id: "meteor-load-saved-btn"
                                  , className: "action-btn secondary"
                                  , disabled: not meteorInfo.hasSavedPlanet
                                  , onClick: capture_ (loadSavedPlanetIntoMeteor setMeteorInfo)
                                  , children: [ R.text "Load Saved" ]
                                  }
                              , R.button
                                  { id: "meteor-load-file-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadFilePlanetIntoMeteor setMeteorInfo)
                                  , children: [ R.text "Load..." ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }

      SunScreen ->
        R.div
          { className: "mode-layout"
          , children:
              [ R.div
                  { className: "viewport-column"
                  , children:
                      [ R.div
                          { className: "canvas-container"
                          , children:
                              [ R.canvas
                                  { id: "sphere-canvas"
                                  , className: "sphere-canvas interactive-canvas"
                                  }
                              ]
                          }
                      , R.div
                          { className: "slider-row"
                          , children:
                              [ R.input
                                  { id: "rotate-slider"
                                  , className: "rotate-slider"
                                  , type: "range"
                                  , min: "-180"
                                  , max: "180"
                                  , step: "1"
                                  , value: show rotationDeg
                                  , onChange: handler targetValue handleRotateChange
                                  }
                              , R.span
                                  { id: "rotation-label"
                                  , className: "rotation-label"
                                  , children: [ R.text (show rotationDeg <> "°") ]
                                  }
                              , R.select
                                  { id: "sun-aspect-select"
                                  , className: "control-select"
                                  , value: sunAspect
                                  , onChange: handler targetValue handleSunAspectChange
                                  , children:
                                      [ R.option
                                          { value: "night_sky"
                                          , children: [ R.text "Night Sky (Celestial Sphere)" ]
                                          }
                                      , R.option
                                          { value: "solar_system"
                                          , children: [ R.text "Planetary System & Orbits (Rocky, Gas Giant, Rings)" ]
                                          }
                                      , R.option
                                          { value: "sky_plants"
                                          , children: [ R.text "Sky, Sunset & Plant Foliage (2D Landscape)" ]
                                          }
                                      , R.option
                                          { value: "home_sun"
                                          , children: [ R.text "Home Sun (3D Photosphere & Size)" ]
                                          }
                                      , R.option
                                          { value: "galaxy"
                                          , children: [ R.text "3D Galaxy & Host Star Location" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Star, Sky Scattering, Sunset & Plant Pigment Inspection" ]
                                  }
                              , R.div
                                  { id: "sun-details-box"
                                  , className: "tile-details-box"
                                  , children: [ R.text sunInfo.selectedReport ]
                                  }
                              ]
                          }
                      ]
                  }
              , R.div
                  { className: "sidebar-column tectonics-sidebar"
                  , children:
                      [ R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Host Galaxy Morphology" ]
                                  }
                              , R.select
                                  { id: "sun-galaxy-select"
                                  , className: "control-select"
                                  , value: show sunGalaxyType
                                  , onChange: handler targetValue handleSunGalaxyTypeChange
                                  , children:
                                      [ R.option
                                          { value: "99"
                                          , children: [ R.text "Random (Any Plausible Galaxy)" ]
                                          }
                                      , R.option
                                          { value: "0"
                                          , children: [ R.text "Spiral Galaxy (Sb/Sc, Milky Way-like)" ]
                                          }
                                      , R.option
                                          { value: "1"
                                          , children: [ R.text "Barred Spiral Galaxy (SBb)" ]
                                          }
                                      , R.option
                                          { value: "2"
                                          , children: [ R.text "Lenticular Disk Galaxy (S0, Smooth Disk)" ]
                                          }
                                      , R.option
                                          { value: "3"
                                          , children: [ R.text "Elliptical Galaxy (E2, Triaxial Spheroid)" ]
                                          }
                                      , R.option
                                          { value: "4"
                                          , children: [ R.text "Irregular Starburst Galaxy (Irr, OB Knots)" ]
                                          }
                                      , R.option
                                          { value: "5"
                                          , children: [ R.text "Globular Cluster (Dense Halo Cluster)" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Galactic Vantage Region" ]
                                  }
                              , R.select
                                  { id: "sun-vantage-select"
                                  , className: "control-select"
                                  , value: show sunVantage
                                  , onChange: handler targetValue handleSunVantageChange
                                  , children:
                                      [ R.option
                                          { value: "99"
                                          , children: [ R.text "Auto (Weighted by Galaxy Structure)" ]
                                          }
                                      , R.option
                                          { value: "0"
                                          , children: [ R.text "Spiral Arm / Star-Forming Knot" ]
                                          }
                                      , R.option
                                          { value: "1"
                                          , children: [ R.text "Inter-Arm Thin Stellar Disk" ]
                                          }
                                      , R.option
                                          { value: "2"
                                          , children: [ R.text "Galactic Bulge / Central Bar" ]
                                          }
                                      , R.option
                                          { value: "3"
                                          , children: [ R.text "Galactic Halo (High Above Disk)" ]
                                          }
                                      , R.option
                                          { value: "4"
                                          , children: [ R.text "Triaxial Spheroid / Cluster Interior" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Home Star (Sun) Spectral Class" ]
                                  }
                              , R.select
                                  { id: "sun-class-select"
                                  , className: "control-select"
                                  , value: show sunStarFilter
                                  , onChange: handler targetValue handleSunStarFilterChange
                                  , children:
                                      [ R.option
                                          { value: "0"
                                          , children: [ R.text "Auto (Habitable Main Sequence)" ]
                                          }
                                      , R.option
                                          { value: "2"
                                          , children: [ R.text "G-Class Yellow Dwarf (0.85–1.1 M☉, Sol-like)" ]
                                          }
                                      , R.option
                                          { value: "3"
                                          , children: [ R.text "K-Class Orange Dwarf (0.55–0.85 M☉)" ]
                                          }
                                      , R.option
                                          { value: "1"
                                          , children: [ R.text "F-Class Yellow-White Dwarf (1.1–1.5 M☉)" ]
                                          }
                                      , R.option
                                          { value: "4"
                                          , children: [ R.text "M-Class Red Dwarf (0.20–0.55 M☉)" ]
                                          }
                                      , R.option
                                          { value: "5"
                                          , children: [ R.text "A-Class Blue-White Star (1.5–2.1 M☉)" ]
                                          }
                                      , R.option
                                          { value: "6"
                                          , children: [ R.text "Evolved Subgiant (IV, 2.1–3.2 R☉)" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "control-group"
                          , children:
                              [ R.label
                                  { className: "control-label"
                                  , children: [ R.text "Atmospheric Gases & Aerosols (Rayleigh / Mie / Absorption)" ]
                                  }
                              , R.select
                                  { id: "sun-atmosphere-select"
                                  , className: "control-select"
                                  , value: show sunAtmosphere
                                  , onChange: handler targetValue handleSunAtmosphereChange
                                  , children:
                                      [ R.option
                                          { value: "0"
                                          , children: [ R.text "N2-O2 + O3 Ozone (Earth-like, 1.0 bar)" ]
                                          }
                                      , R.option
                                          { value: "1"
                                          , children: [ R.text "N2-CH4 Methane-Rich (Red-Absorbing, 1.2 bar)" ]
                                          }
                                      , R.option
                                          { value: "2"
                                          , children: [ R.text "Dense CO2-N2 (High-Pressure, 4.5 bar)" ]
                                          }
                                      , R.option
                                          { value: "3"
                                          , children: [ R.text "N2-CH4 Tholin Haze (Organic Aerosols, 1.5 bar)" ]
                                          }
                                      , R.option
                                          { value: "4"
                                          , children: [ R.text "Thin CO2 + Iron-Oxide Dust (Mars-like, 0.08 bar)" ]
                                          }
                                      , R.option
                                          { value: "5"
                                          , children: [ R.text "H2-He-N2 + H2O Vapor (Hycean, 2.5 bar)" ]
                                          }
                                      , R.option
                                          { value: "99"
                                          , children: [ R.text "Random Plausible Atmosphere" ]
                                          }
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "sun-generate-galaxy-btn"
                                  , className: "action-btn"
                                  , onClick: capture_ (generateSunGalaxy sunGalaxyType sunVantage sunStarFilter setSunInfo)
                                  , children: [ R.text "New Galaxy & Sun" ]
                                  }
                              , R.button
                                  { id: "sun-choose-star-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (chooseNewSunInGalaxy sunStarFilter setSunInfo)
                                  , children: [ R.text "New Sun in Galaxy" ]
                                  }
                              , R.button
                                  { id: "sun-hipparcos-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (loadHipparcosIntoSun setSunInfo)
                                  , children: [ R.text "Sol (Hipparcos)" ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "sun-toggle-plane-btn"
                                  , className: if sunShowGalacticPlane then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleSunGalacticPlane
                                  , children: [ R.text (if sunShowGalacticPlane then "Galactic Plane: On" else "Galactic Plane: Off") ]
                                  }
                              , R.button
                                  { id: "sun-toggle-constellations-btn"
                                  , className: if sunShowConstellations then "action-btn" else "action-btn secondary"
                                  , onClick: capture_ handleToggleSunConstellations
                                  , children: [ R.text (if sunShowConstellations then "Constellations: On" else "Constellations: Off") ]
                                  }
                              ]
                          }
                      , R.div
                          { id: "sun-stats"
                          , className: "grid-stats"
                          , children:
                              [ R.div
                                  { className: "stat-line"
                                  , style: R.css { display: "flex", alignItems: "center", gap: "8px" }
                                  , children:
                                      [ R.span
                                          { style:
                                              R.css
                                                { display: "inline-block"
                                                , width: "16px"
                                                , height: "16px"
                                                , borderRadius: "50%"
                                                , backgroundColor: sunInfo.sunColorCss
                                                , boxShadow: "0 0 8px " <> sunInfo.sunColorCss
                                                , flexShrink: "0"
                                                }
                                          , children: []
                                          }
                                      , R.span
                                          { children:
                                              [ R.text
                                                  ( "Sun: "
                                                      <> sunInfo.sunSpectralType
                                                      <> " ("
                                                      <> sunInfo.sunClassDesc
                                                      <> ") — "
                                                      <> sunInfo.sunTempText
                                                      <> ", "
                                                      <> sunInfo.sunRadiusText
                                                      <> ", "
                                                      <> sunInfo.sunLumText
                                                  )
                                              ]
                                          }
                                      ]
                                  }
                              , R.div
                                  { id: "sun-sky-swatch-line"
                                  , className: "stat-line"
                                  , style: R.css { display: "flex", alignItems: "center", gap: "8px" }
                                  , children:
                                      [ R.span
                                          { style:
                                              R.css
                                                { display: "inline-block"
                                                , width: "16px"
                                                , height: "16px"
                                                , borderRadius: "50%"
                                                , background: "linear-gradient(180deg, " <> sunInfo.skyColorCss <> " 0%, " <> sunInfo.skyHorizonCss <> " 100%)"
                                                , border: "1px solid rgba(255,255,255,0.28)"
                                                , flexShrink: "0"
                                                }
                                          , children: []
                                          }
                                      , R.span
                                          { children:
                                              [ R.text
                                                  ( "Daytime Sky: "
                                                      <> sunInfo.skyColorName
                                                      <> " ("
                                                      <> sunInfo.atmosphereFormula
                                                      <> ")"
                                                  )
                                              ]
                                          }
                                      ]
                                  }
                              , R.div
                                  { id: "sun-sunset-swatch-line"
                                  , className: "stat-line"
                                  , style: R.css { display: "flex", alignItems: "center", gap: "8px" }
                                  , children:
                                      [ R.span
                                          { style:
                                              R.css
                                                { display: "inline-block"
                                                , width: "16px"
                                                , height: "16px"
                                                , borderRadius: "50%"
                                                , background: "linear-gradient(135deg, " <> sunInfo.sunsetColorCss <> " 0%, " <> sunInfo.twilightColorCss <> " 100%)"
                                                , border: "1px solid rgba(255,255,255,0.28)"
                                                , flexShrink: "0"
                                                }
                                          , children: []
                                          }
                                      , R.span
                                          { children:
                                              [ R.text
                                                  ( "Sunset & Twilight: "
                                                      <> sunInfo.sunsetColorName
                                                      <> " — "
                                                      <> sunInfo.sunsetMechanism
                                                  )
                                              ]
                                          }
                                      ]
                                  }
                              , R.div
                                  { id: "sun-plant-swatch-line"
                                  , className: "stat-line"
                                  , style: R.css { display: "flex", alignItems: "center", gap: "8px" }
                                  , children:
                                      [ R.span
                                          { style:
                                              R.css
                                                { display: "inline-block"
                                                , width: "16px"
                                                , height: "16px"
                                                , borderRadius: "50%"
                                                , background: "linear-gradient(135deg, " <> sunInfo.plantHighlightCss <> " 0%, " <> sunInfo.plantColorCss <> " 100%)"
                                                , border: "1px solid rgba(255,255,255,0.28)"
                                                , flexShrink: "0"
                                                }
                                          , children: []
                                          }
                                      , R.span
                                          { children:
                                              [ R.text
                                                  ( "Main Plant Color: "
                                                      <> sunInfo.plantColorName
                                                      <> " — "
                                                      <> sunInfo.plantPigmentDesc
                                                  )
                                              ]
                                          }
                                      ]
                                  }
                              , R.div
                                  { id: "sun-planets-line"
                                  , className: "stat-line"
                                  , children:
                                      [ R.text ("Planets: " <> sunInfo.planetsSummaryText) ]
                                  }
                              , R.div
                                  { id: "sun-summary-line"
                                  , className: "stat-line"
                                  , children:
                                      [ R.text
                                          ( "Galaxy: "
                                              <> sunInfo.galaxyTypeName
                                              <> " — "
                                              <> sunInfo.vantageName
                                              <> " ("
                                              <> show sunInfo.starCount
                                              <> " visible stars)"
                                          )
                                      ]
                                  }
                              ]
                          }
                      , R.div
                          { className: "button-row"
                          , children:
                              [ R.button
                                  { id: "done-btn"
                                  , className: "action-btn secondary"
                                  , onClick: capture_ (setScreen MainMenu)
                                  , children: [ R.text "Done" ]
                                  }
                              ]
                          }
                      ]
                  }
              ]
          }
