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

derive instance eqScreen :: Eq Screen

type GridInfo =
  { size :: Int
  , tileCount :: Int
  }

foreign import initWasm :: Effect Unit -> Effect Unit
foreign import mountGridView :: (GridInfo -> Effect Unit) -> Effect (Effect Unit)
foreign import subdivideGrid :: (GridInfo -> Effect Unit) -> Effect Unit
foreign import rotateGrid :: Number -> Effect Unit

mkApp :: Component Unit
mkApp = do
  component "Tec11App" \_ -> React.do
    wasmReady /\ setWasmReady <- useState' false
    screen /\ setScreen <- useState' MainMenu
    gridInfo /\ setGridInfo <- useState' { size: 0, tileCount: 12 }
    rotationDeg /\ setRotationDeg <- useState' 0

    useEffectOnce do
      initWasm (setWasmReady true)
      pure (pure unit)

    useEffect screen do
      case screen of
        MainMenu -> pure (pure unit)
        GridScreen -> do
          setRotationDeg 0
          mountGridView setGridInfo

    let
      handleOpenGrid :: Effect Unit
      handleOpenGrid = setScreen GridScreen

      handleFiner :: Effect Unit
      handleFiner = subdivideGrid setGridInfo

      handleRotateChange :: Maybe String -> Effect Unit
      handleRotateChange maybeVal = case maybeVal >>= Int.fromString of
        Nothing -> pure unit
        Just deg -> do
          setRotationDeg deg
          rotateGrid (Int.toNumber deg)

      handleDone :: Effect Unit
      handleDone = setScreen MainMenu

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
                          , onClick: capture_ handleOpenGrid
                          , children: [ R.text "Grid" ]
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
                          , onClick: capture_ handleFiner
                          , children: [ R.text "Finer" ]
                          }
                      , R.button
                          { id: "done-btn"
                          , className: "action-btn secondary"
                          , onClick: capture_ handleDone
                          , children: [ R.text "Done" ]
                          }
                      ]
                  }
              ]
          }
