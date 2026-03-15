module Orchestrator.Version
    ( readCargoVersion
    , bumpPatch
    , writeCargoVersion
    ) where

-- | Read the version string from Cargo.toml.
readCargoVersion :: IO String
readCargoVersion = do
    content <- readFile "Cargo.toml"
    case extractVersion (lines content) of
        Just ver -> return ver
        Nothing  -> error "Could not find version in Cargo.toml"

-- | Extract version from 'version = "X.Y.Z"' line.
extractVersion :: [String] -> Maybe String
extractVersion [] = Nothing
extractVersion (line:rest)
    | take 10 (stripSpaces line) == "version = " =
        Just (extractQuoted (drop 10 (stripSpaces line)))
    | otherwise = extractVersion rest

-- | Extract text between first pair of quotes.
extractQuoted :: String -> String
extractQuoted s =
    let afterFirst = drop 1 (dropWhile (/= '"') s)
    in  takeWhile (/= '"') afterFirst

stripSpaces :: String -> String
stripSpaces = dropWhile (== ' ')

-- | Bump the patch component of a semver string (X.Y.Z -> X.Y.(Z+1)).
bumpPatch :: String -> String
bumpPatch ver =
    let parts = splitOn '.' ver
    in case parts of
        [major, minor, patch] ->
            let newPatch = show (read patch + 1 :: Int)
            in  major ++ "." ++ minor ++ "." ++ newPatch
        _ -> error $ "Invalid version format: " ++ ver

-- | Split a string on a delimiter character.
splitOn :: Char -> String -> [String]
splitOn _ "" = [""]
splitOn delim s =
    let (chunk, rest) = break (== delim) s
    in  chunk : case rest of
            [] -> []
            (_:xs) -> splitOn delim xs

-- | Write a new version into Cargo.toml, replacing the existing version line.
writeCargoVersion :: String -> IO ()
writeCargoVersion newVer = do
    content <- readFile "Cargo.toml"
    let newContent = unlines (map (replaceVersionLine newVer) (lines content))
    -- Force read to complete before writing (lazy IO)
    length newContent `seq` writeFile "Cargo.toml" newContent

replaceVersionLine :: String -> String -> String
replaceVersionLine newVer line
    | take 10 (stripSpaces line) == "version = " =
        "version = \"" ++ newVer ++ "\""
    | otherwise = line
