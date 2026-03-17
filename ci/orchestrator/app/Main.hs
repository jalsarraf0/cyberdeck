module Main (main) where

import Orchestrator.Pipeline (dedup, ciGate)
import Orchestrator.Runner   (runPipeline)
import Orchestrator.Version  (readCargoVersion, bumpPatch, writeCargoVersion)
import Orchestrator.Workflow (writeCiWorkflow, writeReleaseWorkflow, writeSecurityWorkflow)
import Orchestrator.Package  (writeRpmSpec, writeDebControl, writePkgbuild, writePackageScript)

import System.Directory      (getCurrentDirectory, setCurrentDirectory, doesDirectoryExist)
import System.Environment    (getArgs)
import System.Exit           (exitFailure, exitSuccess)
import System.IO             (hFlush, stdout, hPutStrLn, stderr)

main :: IO ()
main = do
    args <- getArgs
    case args of
        ["ci"]            -> doCi
        ["generate"]      -> doGenerate
        ["bump"]          -> doBump
        ["release"]       -> doRelease
        ["full"]          -> doFull
        ["--help"]        -> doHelp
        ["-h"]            -> doHelp
        _                 -> doHelp

doHelp :: IO ()
doHelp = do
    putStrLn "cyberdeck CI orchestrator"
    putStrLn ""
    putStrLn "Usage: orchestrator <command>"
    putStrLn ""
    putStrLn "Commands:"
    putStrLn "  ci        Run the CI gate locally (fmt, clippy, test)"
    putStrLn "  generate  Generate clean GitHub Actions workflows"
    putStrLn "  bump      Bump patch version in Cargo.toml"
    putStrLn "  release   Generate packaging files (rpm, deb, pacman, tar.gz)"
    putStrLn "  full      Run full pipeline: ci -> generate -> bump -> release"

-- | Run the local CI gate
doCi :: IO ()
doCi = do
    ensureProjectRoot
    putStrLn "=== Running CI gate ==="
    let pipeline = dedup ciGate
    ok <- runPipeline pipeline
    if ok
        then putStrLn "CI gate PASSED." >> exitSuccess
        else hPutStrLn stderr "CI gate FAILED." >> exitFailure

-- | Generate clean GitHub Actions workflows
doGenerate :: IO ()
doGenerate = do
    ensureProjectRoot
    putStrLn "=== Generating GitHub Actions workflows ==="
    writeCiWorkflow
    writeReleaseWorkflow
    writeSecurityWorkflow
    putStrLn "Workflows written to .github/workflows/"

-- | Bump patch version
doBump :: IO ()
doBump = do
    ensureProjectRoot
    ver <- readCargoVersion
    let newVer = bumpPatch ver
    writeCargoVersion newVer
    putStrLn $ "Version bumped: " ++ ver ++ " -> " ++ newVer

-- | Generate packaging files
doRelease :: IO ()
doRelease = do
    ensureProjectRoot
    ver <- readCargoVersion
    putStrLn $ "=== Generating release packaging for v" ++ ver ++ " ==="
    writeRpmSpec ver
    writeDebControl ver
    writePkgbuild ver
    writePackageScript ver
    putStrLn "Packaging files written to packaging/"

-- | Full pipeline: ci -> generate -> release
doFull :: IO ()
doFull = do
    ensureProjectRoot
    putStrLn "=== Full CI/CD pipeline ==="
    putStrLn ""

    -- Step 1: CI gate
    putStrLn "--- Step 1: CI gate ---"
    let pipeline = dedup ciGate
    ok <- runPipeline pipeline
    if not ok
        then hPutStrLn stderr "CI gate FAILED. Aborting pipeline." >> exitFailure
        else putStrLn "CI gate passed."

    -- Step 2: Generate workflows
    putStrLn ""
    putStrLn "--- Step 2: Generate workflows ---"
    writeCiWorkflow
    writeReleaseWorkflow
    writeSecurityWorkflow
    putStrLn "Workflows generated."

    -- Step 3: Generate packaging
    putStrLn ""
    putStrLn "--- Step 3: Generate packaging ---"
    ver <- readCargoVersion
    writeRpmSpec ver
    writeDebControl ver
    writePkgbuild ver
    writePackageScript ver
    putStrLn $ "Packaging generated for v" ++ ver ++ "."

    putStrLn ""
    putStrLn "=== Pipeline complete ==="
    putStrLn "Next steps:"
    putStrLn "  git add -A && git commit"
    putStrLn "  git push origin main"
    putStrLn $ "  git tag v" ++ ver ++ " && git push origin v" ++ ver

-- | Ensure we are in the project root (contains Cargo.toml)
ensureProjectRoot :: IO ()
ensureProjectRoot = do
    cwd <- getCurrentDirectory
    -- Walk up to find the project root containing Cargo.toml
    let ciDir = cwd ++ "/ci/orchestrator"
    inCiDir <- doesDirectoryExist ciDir
    if inCiDir
        then return ()  -- already in project root
        else do
            -- Maybe we're in ci/orchestrator — go up two levels
            setCurrentDirectory (cwd ++ "/../..")
            newCwd <- getCurrentDirectory
            putStrLn $ "Changed to project root: " ++ newCwd
    hFlush stdout
