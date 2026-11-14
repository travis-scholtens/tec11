module Tec11.SkyVantage
  ( DemographicShareItem
  , TileViewInfo
  , defaultTileViewInfo
  , SkyClockMode(..)
  , SparklineMeta
  , renderTileSkyAndVantageSection
  , renderTileSkyAndVantageBox
  , renderSparklineBox
  , setTileSkyYear
  , setTileSkyDay
  , toggleTileSkyAnimation
  , stepTileVantageDirection
  ) where

import Prelude

import Data.Int as Int
import Data.Maybe (Maybe(..))
import Effect (Effect)
import React.Basic.DOM as R
import React.Basic.DOM.Events (capture_, targetValue)
import React.Basic.Events (handler)
import React.Basic.Hooks (JSX)
import React.Basic.Hooks as React

type DemographicShareItem =
  { id :: Int
  , name :: String
  , popText :: String
  , percentText :: String
  }

type TileViewInfo =
  { hasTile :: Boolean
  , displayIdx :: Int
  , stateId :: Int
  , title :: String
  , locationSummary :: String
  , populationSummary :: String
  , technology :: String
  , nationalities :: Array DemographicShareItem
  , languages :: Array DemographicShareItem
  , flora :: Array String
  , fauna :: Array String
  , skyYear :: Int
  , skyDay :: Int
  , skyDaysPerYear :: Int
  , skyLatLonSummary :: String
  , skyAnimated :: Boolean
  , vantageBearingSummary :: String
  , vantageNeighborSummary :: String
  }

defaultTileViewInfo :: TileViewInfo
defaultTileViewInfo =
  { hasTile: false
  , displayIdx: -1
  , stateId: -1
  , title: ""
  , locationSummary: ""
  , populationSummary: ""
  , technology: ""
  , nationalities: []
  , languages: []
  , flora: []
  , fauna: []
  , skyYear: 0
  , skyDay: 0
  , skyDaysPerYear: 365
  , skyLatLonSummary: ""
  , skyAnimated: true
  , vantageBearingSummary: ""
  , vantageNeighborSummary: ""
  }

data SkyClockMode
  = InteractiveSkyClock
  | SyncedSimClock String

type SparklineMeta =
  { canvasId :: String
  , title :: String
  , subtitle :: String
  , minLabel :: String
  , maxLabel :: String
  , currentLabel :: String
  }

foreign import setTileSkyYear :: Int -> Effect Unit
foreign import setTileSkyDay :: Int -> Effect Unit
foreign import toggleTileSkyAnimation :: Effect Unit
foreign import stepTileVantageDirection :: Int -> Effect Unit

renderSparklineBox :: SparklineMeta -> JSX
renderSparklineBox meta =
  R.div
    { style:
        R.css
          { marginTop: "7px"
          , padding: "7px 8px"
          , borderRadius: "6px"
          , border: "1px solid #334155"
          , backgroundColor: "rgba(15, 23, 42, 0.78)"
          }
    , children:
        [ R.div
            { style:
                R.css
                  { display: "flex"
                  , justifyContent: "space-between"
                  , alignItems: "baseline"
                  , gap: "8px"
                  , marginBottom: "3px"
                  }
            , children:
                [ R.span
                    { style: R.css { fontSize: "0.75rem", fontWeight: "700", color: "#38bdf8" }
                    , children: [ R.text meta.title ]
                    }
                , R.span
                    { style: R.css { fontSize: "0.72rem", fontWeight: "600", color: "#fde047" }
                    , children: [ R.text meta.currentLabel ]
                    }
                ]
            }
        , if meta.subtitle == "" then
            React.empty
          else
            R.div
              { style: R.css { fontSize: "0.69rem", color: "#94a3b8", marginBottom: "4px" }
              , children: [ R.text meta.subtitle ]
              }
        , R.canvas
            { id: meta.canvasId
            , style:
                R.css
                  { width: "100%"
                  , height: "94px"
                  , display: "block"
                  , borderRadius: "5px"
                  , border: "1px solid #1e293b"
                  , background: "#050b16"
                  }
            }
        , R.div
            { style:
                R.css
                  { display: "flex"
                  , justifyContent: "space-between"
                  , fontSize: "0.69rem"
                  , color: "#cbd5e1"
                  , marginTop: "3px"
                  }
            , children:
                [ R.span { children: [ R.text meta.minLabel ] }
                , R.span { children: [ R.text meta.maxLabel ] }
                ]
            }
        ]
    }

renderTileSkyAndVantageBox :: TileViewInfo -> JSX
renderTileSkyAndVantageBox = renderTileSkyAndVantageSection InteractiveSkyClock

renderTileSkyAndVantageSection :: SkyClockMode -> TileViewInfo -> JSX
renderTileSkyAndVantageSection clockMode tile =
  if not tile.hasTile then
    React.empty
  else
    R.div
      { id: "tile-sky-box"
      , style:
          R.css
            { marginTop: "8px"
            , paddingTop: "7px"
            , borderTop: "1px solid #334155"
            }
      , children:
          [ R.div
              { style:
                  R.css
                    { display: "flex"
                    , justifyContent: "space-between"
                    , alignItems: "center"
                    , gap: "6px"
                    , marginBottom: "4px"
                    }
              , children:
                  [ R.div
                      { style: R.css { fontSize: "0.76rem", fontWeight: "700", color: "#fde047" }
                      , children:
                          [ R.text
                              ( "Local Sky & Vantage ("
                                  <> tile.skyLatLonSummary
                                  <> ")"
                              )
                          ]
                      }
                  , case clockMode of
                      InteractiveSkyClock ->
                        R.button
                          { id: "tile-sky-anim-btn"
                          , className: "action-btn secondary"
                          , style: R.css { padding: "1px 8px", fontSize: "0.72rem", width: "auto" }
                          , onClick: capture_ toggleTileSkyAnimation
                          , children:
                              [ R.text (if tile.skyAnimated then "⏸ Pause Sky" else "▶ Animate Sky") ]
                          }
                      SyncedSimClock badge ->
                        R.span
                          { id: "tile-sky-sync-badge"
                          , style:
                              R.css
                                { fontSize: "0.69rem"
                                , fontWeight: "600"
                                , color: "#7dd3fc"
                                , padding: "1px 6px"
                                , borderRadius: "4px"
                                , border: "1px solid #0369a1"
                                , backgroundColor: "rgba(12, 74, 110, 0.45)"
                                , whiteSpace: "nowrap"
                                }
                          , children: [ R.text badge ]
                          }
                  ]
              }
          , case clockMode of
              InteractiveSkyClock ->
                React.fragment
                  [ R.div
                      { style:
                          R.css
                            { display: "flex"
                            , alignItems: "center"
                            , justifyContent: "space-between"
                            , gap: "8px"
                            , marginBottom: "5px"
                            }
                      , children:
                          [ R.label
                              { style: R.css { fontSize: "0.74rem", color: "#cbd5e1", whiteSpace: "nowrap" }
                              , children: [ R.text "Year (orbital cycle):" ]
                              }
                          , R.input
                              { id: "tile-sky-year-input"
                              , type: "number"
                              , min: "0"
                              , step: "1"
                              , value: show tile.skyYear
                              , style:
                                  R.css
                                    { width: "86px"
                                    , padding: "2px 6px"
                                    , fontSize: "0.76rem"
                                    , background: "#0f172a"
                                    , color: "#f8fafc"
                                    , border: "1px solid #475569"
                                    , borderRadius: "4px"
                                    }
                              , onChange:
                                  handler targetValue \mv -> case mv >>= Int.fromString of
                                    Just y -> setTileSkyYear (max 0 y)
                                    Nothing -> pure unit
                              }
                          ]
                      }
                  , R.div
                      { style: R.css { marginBottom: "6px" }
                      , children:
                          [ R.div
                              { style:
                                  R.css
                                    { display: "flex"
                                    , justifyContent: "space-between"
                                    , fontSize: "0.73rem"
                                    , color: "#cbd5e1"
                                    , marginBottom: "2px"
                                    }
                              , children:
                                  [ R.span
                                      { id: "tile-sky-day-label"
                                      , children:
                                          [ R.text
                                              ( "Day in Year: Day "
                                                  <> show tile.skyDay
                                                  <> " / "
                                                  <> show (max 1 (tile.skyDaysPerYear - 1))
                                              )
                                          ]
                                      }
                                  , R.span
                                      { id: "tile-sky-time-label"
                                      , style: R.css { color: "#93c5fd" }
                                      , children: [ R.text "Sun · Moons · Planets · Stars" ]
                                      }
                                  ]
                              }
                          , R.input
                              { id: "tile-sky-day-slider"
                              , type: "range"
                              , min: "0"
                              , max: show (max 1 (tile.skyDaysPerYear - 1))
                              , step: "1"
                              , value: show tile.skyDay
                              , style: R.css { width: "100%", cursor: "pointer" }
                              , onChange:
                                  handler targetValue \mv -> case mv >>= Int.fromString of
                                    Just d -> setTileSkyDay (max 0 d)
                                    Nothing -> pure unit
                              }
                          ]
                      }
                  ]
              SyncedSimClock _ ->
                R.div
                  { style:
                      R.css
                        { display: "flex"
                        , justifyContent: "space-between"
                        , fontSize: "0.72rem"
                        , color: "#cbd5e1"
                        , marginBottom: "5px"
                        }
                  , children:
                      [ R.span
                          { id: "tile-sky-day-label"
                          , children:
                              [ R.text
                                  ( "Sim Day "
                                      <> show tile.skyDay
                                      <> " / "
                                      <> show (max 1 (tile.skyDaysPerYear - 1))
                                  )
                              ]
                          }
                      , R.span
                          { id: "tile-sky-time-label"
                          , style: R.css { color: "#93c5fd" }
                          , children: [ R.text "Synced Local Solar Time" ]
                          }
                      ]
                  }
          , R.div
              { style:
                  R.css
                    { display: "flex"
                    , flexDirection: "row"
                    , gap: "8px"
                    , alignItems: "flex-end"
                    }
              , children:
                  [ R.div
                      { style: R.css { flex: "1 1 0", minWidth: "0" }
                      , children:
                          [ R.div
                              { style:
                                  R.css
                                    { fontSize: "0.70rem"
                                    , color: "#94a3b8"
                                    , textAlign: "center"
                                    , height: "20px"
                                    , lineHeight: "20px"
                                    , marginBottom: "3px"
                                    , whiteSpace: "nowrap"
                                    , overflow: "hidden"
                                    , textOverflow: "ellipsis"
                                    }
                              , children: [ R.text "Overhead Sky Dome" ]
                              }
                          , R.canvas
                              { id: "tile-sky-canvas"
                              , style:
                                  R.css
                                    { width: "100%"
                                    , height: "224px"
                                    , display: "block"
                                    , borderRadius: "6px"
                                    , border: "1px solid #334155"
                                    , background: "#040914"
                                    }
                              }
                          ]
                      }
                  , R.div
                      { style: R.css { flex: "0 0 200px", width: "200px" }
                      , children:
                          [ R.div
                              { style:
                                  R.css
                                    { display: "flex"
                                    , alignItems: "center"
                                    , justifyContent: "space-between"
                                    , gap: "4px"
                                    , height: "20px"
                                    , marginBottom: "3px"
                                    }
                              , children:
                                  [ R.button
                                      { id: "tile-vantage-prev-btn"
                                      , className: "action-btn secondary"
                                      , style:
                                          R.css
                                            { padding: "0px 7px"
                                            , height: "20px"
                                            , lineHeight: "18px"
                                            , fontSize: "0.70rem"
                                            , width: "auto"
                                            , minWidth: "24px"
                                            }
                                      , onClick: capture_ (stepTileVantageDirection (-1))
                                      , children: [ R.text "◀" ]
                                      }
                                  , R.span
                                      { id: "tile-vantage-dir-label"
                                      , style:
                                          R.css
                                            { fontSize: "0.69rem"
                                            , fontWeight: "700"
                                            , color: "#bae6fd"
                                            , textAlign: "center"
                                            , whiteSpace: "nowrap"
                                            , overflow: "hidden"
                                            , textOverflow: "ellipsis"
                                            , flex: "1"
                                            }
                                      , children: [ R.text tile.vantageBearingSummary ]
                                      }
                                  , R.button
                                      { id: "tile-vantage-next-btn"
                                      , className: "action-btn secondary"
                                      , style:
                                          R.css
                                            { padding: "0px 7px"
                                            , height: "20px"
                                            , lineHeight: "18px"
                                            , fontSize: "0.70rem"
                                            , width: "auto"
                                            , minWidth: "24px"
                                            }
                                      , onClick: capture_ (stepTileVantageDirection 1)
                                      , children: [ R.text "▶" ]
                                      }
                                  ]
                              }
                          , R.canvas
                              { id: "tile-vantage-canvas"
                              , style:
                                  R.css
                                    { width: "200px"
                                    , height: "224px"
                                    , display: "block"
                                    , borderRadius: "6px"
                                    , border: "1px solid #334155"
                                    , background: "#040914"
                                    }
                              }
                          ]
                      }
                  ]
              }
          , R.div
              { id: "tile-vantage-summary-label"
              , style:
                  R.css
                    { fontSize: "0.71rem"
                    , color: "#cbd5e1"
                    , marginTop: "4px"
                    }
              , children: [ R.text tile.vantageNeighborSummary ]
              }
          , R.div
              { style: R.css { fontSize: "0.69rem", color: "#94a3b8", marginTop: "2px" }
              , children:
                  [ R.text "Gold: Sun | Silver: Moons | Colored: Planets | Cyan: Constellations | Vantage: 100° FOV, Zenith (+90°) to Horizon" ]
              }
          ]
      }
