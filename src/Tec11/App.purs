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
  | ClimateScreen
  | TectonicsScreen

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

foreign import initWasm :: Effect Unit -> Effect Unit

-- Grid mode FFI
foreign import mountGridView :: (GridInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import subdivideGrid :: (GridInfo -> Effect Unit) -> Effect Unit

-- Pangea mode FFI
foreign import mountPangeaView :: Int -> Int -> String -> (PangeaInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import regeneratePangea :: Int -> Int -> (PangeaInfo -> Effect Unit) -> Effect Unit
foreign import setPangeaRenderMode :: String -> Effect Unit

-- Climate mode FFI
foreign import mountClimateView :: Int -> Int -> String -> Int -> Effect (Effect Unit)
foreign import setClimateAttribute :: String -> Effect Unit
foreign import setClimateSeason :: Int -> Effect Unit
foreign import setClimateParams :: Int -> Int -> Effect Unit

-- Tectonics mode FFI
foreign import mountTectonicsView :: Int -> Int -> String -> (TectonicsInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import stepTectonics :: (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import toggleTectonicsRunning :: (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import resetTectonics :: Int -> Int -> (TectonicsInfo -> Effect Unit) -> Effect Unit
foreign import setTectonicsAspect :: String -> Effect Unit

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

    -- Climate mode state
    climateAttribute /\ setClimateAttributeState <- useState' "koeppen"
    climateSeason /\ setClimateSeasonState <- useState' 0
    climateGlaciation /\ setClimateGlaciationState <- useState' 50
    climateCells /\ setClimateCellsState <- useState' 3

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
        ClimateScreen -> do
          setRotationDeg 0
          mountClimateView climateCells climateGlaciation climateAttribute climateSeason
        TectonicsScreen -> do
          setRotationDeg 0
          mountTectonicsView tectonicsSize tectonicsLandPct tectonicsAspect setTectonicsInfo

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
                                  { className: "stat-line"
                                  , children: [ R.text "Earth (ETOPO2)" ]
                                  }
                              , R.div
                                  { className: "stat-line"
                                  , children: [ R.text "7,292 tiles" ]
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
