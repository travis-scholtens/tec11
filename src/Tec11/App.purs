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
                          { id: "grid-btn"
                          , className: "menu-btn"
                          , disabled: not wasmReady
                          , onClick: capture_ (setScreen GridScreen)
                          , children: [ R.text "Grid" ]
                          }
                      , R.button
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
                          { id: "tectonics-btn"
                          , className: "menu-btn"
                          , disabled: not wasmReady
                          , onClick: capture_ (setScreen TectonicsScreen)
                          , children: [ R.text "Tectonics" ]
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
