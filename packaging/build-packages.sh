#!/usr/bin/env bash
set -euo pipefail

# Build Linux distribution packages for cyberdeck
# Usage: build-packages.sh [rpm|deb|pacman|all]

VERSION="0.1.7"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/output"
BINARY="${SCRIPT_DIR}/../release-assets/cyberdeck"

mkdir -p "${OUTPUT_DIR}"

if [ ! -f "${BINARY}" ]; then
    # Fallback: look for binary in dist/
    BINARY="${SCRIPT_DIR}/../dist/cyberdeck"
    if [ ! -f "${BINARY}" ]; then
        echo "ERROR: cyberdeck binary not found"
        exit 1
    fi
fi

build_rpm() {
    echo "Building RPM..."
    local rpmbuild_dir
    rpmbuild_dir="$(mktemp -d)"
    mkdir -p "${rpmbuild_dir}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    cp "${BINARY}" "${rpmbuild_dir}/SOURCES/cyberdeck"
    cp "${SCRIPT_DIR}/cyberdeck.spec" "${rpmbuild_dir}/SPECS/"
    rpmbuild --define "_topdir ${rpmbuild_dir}" -bb "${rpmbuild_dir}/SPECS/cyberdeck.spec"
    cp "${rpmbuild_dir}"/RPMS/*/*.rpm "${OUTPUT_DIR}/" 2>/dev/null || true
    rm -rf "${rpmbuild_dir}"
    echo "RPM built."
}

build_deb() {
    echo "Building DEB..."
    local deb_dir
    deb_dir="$(mktemp -d)"
    mkdir -p "${deb_dir}/usr/bin"
    mkdir -p "${deb_dir}/DEBIAN"
    cp "${BINARY}" "${deb_dir}/usr/bin/cyberdeck"
    chmod 0755 "${deb_dir}/usr/bin/cyberdeck"
    cp "${SCRIPT_DIR}/debian/DEBIAN/control" "${deb_dir}/DEBIAN/control"
    dpkg-deb --build "${deb_dir}" "${OUTPUT_DIR}/cyberdeck_${VERSION}-1_amd64.deb"
    rm -rf "${deb_dir}"
    echo "DEB built."
}

build_pacman() {
    echo "Building Pacman package..."
    local pkg_dir
    pkg_dir="$(mktemp -d)"
    cp "${SCRIPT_DIR}/PKGBUILD" "${pkg_dir}/"
    mkdir -p "${pkg_dir}/src"
    cp "${BINARY}" "${pkg_dir}/src/cyberdeck"
    cd "${pkg_dir}"
    if command -v makepkg >/dev/null 2>&1; then
        makepkg --skipchecksums --nodeps 2>/dev/null || true
        cp "${pkg_dir}"/*.pkg.tar.zst "${OUTPUT_DIR}/" 2>/dev/null || true
    else
        echo "WARN: makepkg not available, creating manual tar.zst"
        mkdir -p pkg/usr/bin
        cp src/cyberdeck pkg/usr/bin/
        chmod 0755 pkg/usr/bin/cyberdeck
        tar --zstd -cf "${OUTPUT_DIR}/cyberdeck-${VERSION}-1-x86_64.pkg.tar.zst" -C pkg .
    fi
    rm -rf "${pkg_dir}"
    echo "Pacman package built."
}

case "${1:-all}" in
    rpm)    build_rpm ;;
    deb)    build_deb ;;
    pacman) build_pacman ;;
    all)
        build_rpm
        build_deb
        build_pacman
        ;;
    *)
        echo "Usage: $0 [rpm|deb|pacman|all]"
        exit 1
        ;;
esac

echo "Packages available in ${OUTPUT_DIR}/"
ls -la "${OUTPUT_DIR}/"
