module Orchestrator.Pipeline
    ( Step(..)
    , stepCommand
    , stepLabel
    , dedup
    , ciGate
    ) where

import qualified Data.Set as Set

-- | A CI pipeline step.
data Step
    = FmtCheck
    | ClippyLint
    | TestAll
    | BuildRelease
    | Regression
    | CargoAudit
    | CargoDeny
    | Gitleaks
    deriving (Eq, Ord, Show)

-- | The shell command for each step.
stepCommand :: Step -> String
stepCommand FmtCheck     = "cargo fmt --all --check"
stepCommand ClippyLint   = "cargo clippy --workspace --all-targets -- -D warnings"
stepCommand TestAll       = "cargo test --all-targets"
stepCommand BuildRelease  = "cargo build --release --locked"
stepCommand Regression    = "./scripts/regression.sh"
stepCommand CargoAudit    = "cargo audit"
stepCommand CargoDeny     = "cargo deny check --config deny.toml --hide-inclusion-graph bans licenses sources"
stepCommand Gitleaks      = "gitleaks detect --source . --redact --verbose --exit-code 1"

-- | Human-readable label for each step.
stepLabel :: Step -> String
stepLabel FmtCheck     = "Formatting check"
stepLabel ClippyLint   = "Clippy lint"
stepLabel TestAll       = "Unit & integration tests"
stepLabel BuildRelease  = "Release build"
stepLabel Regression    = "Regression suite"
stepLabel CargoAudit    = "Dependency audit"
stepLabel CargoDeny     = "License & source check"
stepLabel Gitleaks      = "Secret scan"

-- | Deduplicate a pipeline, preserving order of first occurrence.
dedup :: [Step] -> [Step]
dedup = go Set.empty
  where
    go _ [] = []
    go seen (x:xs)
        | Set.member x seen = go seen xs
        | otherwise         = x : go (Set.insert x seen) xs

-- | The standard CI gate pipeline.
ciGate :: [Step]
ciGate =
    [ FmtCheck
    , ClippyLint
    , TestAll
    ]
