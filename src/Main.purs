module Main where

import Prelude

import Data.Maybe (Maybe(..))
import Effect (Effect)
import Effect.Exception (throw)
import React.Basic.DOM.Client (createRoot, renderRoot)
import Tec11.App (mkApp)
import Web.DOM.NonElementParentNode (getElementById)
import Web.HTML (window)
import Web.HTML.HTMLDocument (toNonElementParentNode)
import Web.HTML.Window (document)

main :: Effect Unit
main = do
  doc <- document =<< window
  maybeContainer <- getElementById "app" (toNonElementParentNode doc)
  case maybeContainer of
    Nothing -> throw "Could not find #app container element"
    Just container -> do
      app <- mkApp
      root <- createRoot container
      renderRoot root (app unit)
