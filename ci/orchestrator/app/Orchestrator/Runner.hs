module Orchestrator.Runner
    ( runPipeline
    , runStep
    ) where

import Orchestrator.Pipeline (Step, stepCommand, stepLabel)

import System.Exit    (ExitCode(..))
import System.Process (system)
import System.IO      (hFlush, stdout)

-- | Run a single step and return whether it succeeded.
runStep :: Step -> IO Bool
runStep step = do
    putStrLn $ "  [RUN] " ++ stepLabel step
    putStrLn $ "        " ++ stepCommand step
    hFlush stdout
    exitCode <- system (stepCommand step)
    case exitCode of
        ExitSuccess -> do
            putStrLn $ "  [OK]  " ++ stepLabel step
            return True
        ExitFailure code -> do
            putStrLn $ "  [FAIL] " ++ stepLabel step ++ " (exit " ++ show code ++ ")"
            return False

-- | Run a pipeline of steps sequentially. Stops on first failure.
runPipeline :: [Step] -> IO Bool
runPipeline [] = return True
runPipeline (step:rest) = do
    ok <- runStep step
    if ok
        then runPipeline rest
        else return False
