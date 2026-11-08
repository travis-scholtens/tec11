{ name = "tec11"
, dependencies =
  [ "aff"
  , "aff-promise"
  , "console"
  , "effect"
  , "exceptions"
  , "maybe"
  , "prelude"
  , "react-basic"
  , "react-basic-dom"
  , "react-basic-hooks"
  , "web-dom"
  , "web-html"
  ]
, packages = ./packages.dhall
, sources = [ "src/**/*.purs" ]
}
