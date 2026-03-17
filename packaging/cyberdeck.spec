Name:           cyberdeck
Version:        0.1.7
Release:        1%{?dist}
Summary:        Cyberpunk SSH key management TUI
License:        Apache-2.0
URL:            https://github.com/jalsarraf0/cyberdeck

%description
Cyberdeck is a terminal UI for SSH key management, key exchange,
and remote command execution with a cyberpunk aesthetic.

%install
mkdir -p %{buildroot}%{_bindir}
install -m 0755 %{_sourcedir}/cyberdeck %{buildroot}%{_bindir}/cyberdeck

%files
%{_bindir}/cyberdeck

%changelog
* Sun Mar 15 2026 CI <ci@cyberdeck> - 0.1.7-1
- Automated release build
