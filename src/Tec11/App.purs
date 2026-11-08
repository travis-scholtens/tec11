module Tec11.App
  ( mkApp
  ) where

import Prelude

import Data.Tuple.Nested ((/\))
import Effect (Effect)
import React.Basic.DOM as R
import React.Basic.DOM.Events (capture_)
import React.Basic.Hooks (Component, component, useEffectOnce, useState')
import React.Basic.Hooks as React

foreign import initWasmAndScene :: Effect Unit -> Effect (Effect Unit)
foreign import callRustHello :: String -> Effect String

mkApp :: Component Unit
mkApp = do
  component "Tec11App" \_ -> React.do
    wasmReady /\ setWasmReady <- useState' false
    wasmOutput /\ setWasmOutput <- useState' "Click \"Hello\" to call into the Rust Wasm backend."

    useEffectOnce do
      initWasmAndScene (setWasmReady true)

    let
      handleHelloClick :: Effect Unit
      handleHelloClick = do
        msg <- callRustHello "Tec11 Explorer"
        setWasmOutput msg

    pure $ R.div
      { className: "app-shell"
      , children:
          [ R.div
              { className: "control-panel"
              , children:
                  [ R.h1_ [ R.text "Tec11" ]
                  , R.p
                      { className: "subtitle"
                      , children: [ R.text "PureScript + react-basic + Three.js + Rust WebAssembly" ]
                      }
                  , R.div
                      { className: "status-row"
                      , children:
                          [ R.span
                              { className: if wasmReady then "badge ready" else "badge loading"
                              , children: [ R.text (if wasmReady then "Wasm Ready" else "Loading Wasm...") ]
                              }
                          ]
                      }
                  , R.button
                      { id: "hello-btn"
                      , className: "hello-btn"
                      , disabled: not wasmReady
                      , onClick: capture_ handleHelloClick
                      , children: [ R.text "Hello" ]
                      }
                  , R.div
                      { id: "wasm-output"
                      , className: "wasm-output"
                      , children: [ R.text wasmOutput ]
                      }
                  ]
              }
          , R.div
              { className: "viewport-panel"
              , children:
                  [ R.canvas
                      { id: "three-canvas"
                      , className: "three-canvas"
                      }
                  ]
              }
          ]
      }
