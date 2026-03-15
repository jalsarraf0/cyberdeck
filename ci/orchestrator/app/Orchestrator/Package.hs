module Orchestrator.Package
    ( writeRpmSpec
    , writeDebControl
    , writePkgbuild
    , writePackageScript
    ) where

import System.Directory (createDirectoryIfMissing)

pkgDir :: FilePath
pkgDir = "packaging"

ensureDir :: IO ()
ensureDir = createDirectoryIfMissing True pkgDir

-- | Write an RPM spec file.
writeRpmSpec :: String -> IO ()
writeRpmSpec ver = do
    ensureDir
    writeFile (pkgDir ++ "/cyberdeck.spec") (rpmSpec ver)
    putStrLn "  wrote cyberdeck.spec"

-- | Write a Debian control file.
writeDebControl :: String -> IO ()
writeDebControl ver = do
    ensureDir
    createDirectoryIfMissing True (pkgDir ++ "/debian/DEBIAN")
    writeFile (pkgDir ++ "/debian/DEBIAN/control") (debControl ver)
    putStrLn "  wrote debian/DEBIAN/control"

-- | Write a PKGBUILD for Arch/Pacman.
writePkgbuild :: String -> IO ()
writePkgbuild ver = do
    ensureDir
    writeFile (pkgDir ++ "/PKGBUILD") (pkgbuild ver)
    putStrLn "  wrote PKGBUILD"

-- | Write the build-packages.sh script.
writePackageScript :: String -> IO ()
writePackageScript ver = do
    ensureDir
    writeFile (pkgDir ++ "/build-packages.sh") (buildScript ver)
    putStrLn "  wrote build-packages.sh"

rpmSpec :: String -> String
rpmSpec ver = unlines
    [ "Name:           cyberdeck"
    , "Version:        " ++ ver
    , "Release:        1%{?dist}"
    , "Summary:        Cyberpunk SSH key management TUI"
    , "License:        Apache-2.0"
    , "URL:            https://github.com/jalsarraf0/cyberdeck"
    , ""
    , "%description"
    , "Cyberdeck is a terminal UI for SSH key management, key exchange,"
    , "and remote command execution with a cyberpunk aesthetic."
    , ""
    , "%install"
    , "mkdir -p %{buildroot}%{_bindir}"
    , "install -m 0755 %{_sourcedir}/cyberdeck %{buildroot}%{_bindir}/cyberdeck"
    , ""
    , "%files"
    , "%{_bindir}/cyberdeck"
    , ""
    , "%changelog"
    , "* " ++ "Sun Mar 15 2026 CI <ci@cyberdeck> - " ++ ver ++ "-1"
    , "- Automated release build"
    ]

debControl :: String -> String
debControl ver = unlines
    [ "Package: cyberdeck"
    , "Version: " ++ ver ++ "-1"
    , "Section: utils"
    , "Priority: optional"
    , "Architecture: amd64"
    , "Maintainer: cyberdeck <ci@cyberdeck>"
    , "Description: Cyberpunk SSH key management TUI"
    , " Cyberdeck is a terminal UI for SSH key management, key exchange,"
    , " and remote command execution with a cyberpunk aesthetic."
    , " ."
    , " Features include key generation, SSH config import, key health"
    , " auditing, and remote operations."
    ]

pkgbuild :: String -> String
pkgbuild ver = unlines
    [ "# Maintainer: cyberdeck CI"
    , "pkgname=cyberdeck"
    , "pkgver=" ++ ver
    , "pkgrel=1"
    , "pkgdesc='Cyberpunk SSH key management TUI'"
    , "arch=('x86_64')"
    , "url='https://github.com/jalsarraf0/cyberdeck'"
    , "license=('Apache-2.0')"
    , "depends=('openssh')"
    , ""
    , "package() {"
    , "    install -Dm755 \"$srcdir/cyberdeck\" \"$pkgdir/usr/bin/cyberdeck\""
    , "}"
    ]

buildScript :: String -> String
buildScript ver = unlines
    [ "#!/usr/bin/env bash"
    , "set -euo pipefail"
    , ""
    , "# Build Linux distribution packages for cyberdeck"
    , "# Usage: build-packages.sh [rpm|deb|pacman|all]"
    , ""
    , "VERSION=\"" ++ ver ++ "\""
    , "SCRIPT_DIR=\"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)\""
    , "OUTPUT_DIR=\"${SCRIPT_DIR}/output\""
    , "BINARY=\"${SCRIPT_DIR}/../release-assets/cyberdeck\""
    , ""
    , "mkdir -p \"${OUTPUT_DIR}\""
    , ""
    , "if [ ! -f \"${BINARY}\" ]; then"
    , "    # Fallback: look for binary in dist/"
    , "    BINARY=\"${SCRIPT_DIR}/../dist/cyberdeck\""
    , "    if [ ! -f \"${BINARY}\" ]; then"
    , "        echo \"ERROR: cyberdeck binary not found\""
    , "        exit 1"
    , "    fi"
    , "fi"
    , ""
    , "build_rpm() {"
    , "    echo \"Building RPM...\""
    , "    local rpmbuild_dir"
    , "    rpmbuild_dir=\"$(mktemp -d)\""
    , "    mkdir -p \"${rpmbuild_dir}\"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}"
    , "    cp \"${BINARY}\" \"${rpmbuild_dir}/SOURCES/cyberdeck\""
    , "    cp \"${SCRIPT_DIR}/cyberdeck.spec\" \"${rpmbuild_dir}/SPECS/\""
    , "    rpmbuild --define \"_topdir ${rpmbuild_dir}\" -bb \"${rpmbuild_dir}/SPECS/cyberdeck.spec\""
    , "    cp \"${rpmbuild_dir}\"/RPMS/*/*.rpm \"${OUTPUT_DIR}/\" 2>/dev/null || true"
    , "    rm -rf \"${rpmbuild_dir}\""
    , "    echo \"RPM built.\""
    , "}"
    , ""
    , "build_deb() {"
    , "    echo \"Building DEB...\""
    , "    local deb_dir"
    , "    deb_dir=\"$(mktemp -d)\""
    , "    mkdir -p \"${deb_dir}/usr/bin\""
    , "    mkdir -p \"${deb_dir}/DEBIAN\""
    , "    cp \"${BINARY}\" \"${deb_dir}/usr/bin/cyberdeck\""
    , "    chmod 0755 \"${deb_dir}/usr/bin/cyberdeck\""
    , "    cp \"${SCRIPT_DIR}/debian/DEBIAN/control\" \"${deb_dir}/DEBIAN/control\""
    , "    dpkg-deb --build \"${deb_dir}\" \"${OUTPUT_DIR}/cyberdeck_${VERSION}-1_amd64.deb\""
    , "    rm -rf \"${deb_dir}\""
    , "    echo \"DEB built.\""
    , "}"
    , ""
    , "build_pacman() {"
    , "    echo \"Building Pacman package...\""
    , "    local pkg_dir"
    , "    pkg_dir=\"$(mktemp -d)\""
    , "    cp \"${SCRIPT_DIR}/PKGBUILD\" \"${pkg_dir}/\""
    , "    mkdir -p \"${pkg_dir}/src\""
    , "    cp \"${BINARY}\" \"${pkg_dir}/src/cyberdeck\""
    , "    cd \"${pkg_dir}\""
    , "    if command -v makepkg >/dev/null 2>&1; then"
    , "        makepkg --skipchecksums --nodeps 2>/dev/null || true"
    , "        cp \"${pkg_dir}\"/*.pkg.tar.zst \"${OUTPUT_DIR}/\" 2>/dev/null || true"
    , "    else"
    , "        echo \"WARN: makepkg not available, creating manual tar.zst\""
    , "        mkdir -p pkg/usr/bin"
    , "        cp src/cyberdeck pkg/usr/bin/"
    , "        chmod 0755 pkg/usr/bin/cyberdeck"
    , "        tar --zstd -cf \"${OUTPUT_DIR}/cyberdeck-${VERSION}-1-x86_64.pkg.tar.zst\" -C pkg ."
    , "    fi"
    , "    rm -rf \"${pkg_dir}\""
    , "    echo \"Pacman package built.\""
    , "}"
    , ""
    , "case \"${1:-all}\" in"
    , "    rpm)    build_rpm ;;"
    , "    deb)    build_deb ;;"
    , "    pacman) build_pacman ;;"
    , "    all)"
    , "        build_rpm"
    , "        build_deb"
    , "        build_pacman"
    , "        ;;"
    , "    *)"
    , "        echo \"Usage: $0 [rpm|deb|pacman|all]\""
    , "        exit 1"
    , "        ;;"
    , "esac"
    , ""
    , "echo \"Packages available in ${OUTPUT_DIR}/\""
    , "ls -la \"${OUTPUT_DIR}/\""
    ]
