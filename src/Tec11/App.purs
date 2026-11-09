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

foreign import initWasm :: Effect Unit -> Effect Unit

-- Grid mode FFI
foreign import mountGridView :: (GridInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import subdivideGrid :: (GridInfo -> Effect Unit) -> Effect Unit

-- Pangea mode FFI
foreign import mountPangeaView :: Int -> Int -> String -> (PangeaInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import regeneratePangea :: Int -> Int -> (PangeaInfo -> Effect Unit) -> Effect Unit
foreign import setPangeaRenderMode :: String -> Effect Unit

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
