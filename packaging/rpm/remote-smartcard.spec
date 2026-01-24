%global debug_package %{nil}

Name:           remote-smartcard
Version:        0.1.0
Release:        1%{?dist}
Summary:        Remote Smartcard - Use local smartcards on remote servers

License:        MIT
URL:            https://github.com/alun-hub/remote-smartcard
Source0:        %{name}-%{version}.tar.gz

# Build requirements
BuildRequires:  cargo >= 1.70
BuildRequires:  rust >= 1.70
BuildRequires:  protobuf-compiler
BuildRequires:  pcsc-lite-devel
BuildRequires:  systemd-rpm-macros
BuildRequires:  gcc
BuildRequires:  make

# For Fedora 39+/RHEL 9+/Rocky 9+
%if 0%{?fedora} >= 39 || 0%{?rhel} >= 9
BuildRequires:  rust-packaging
%endif

%description
Remote Smartcard allows you to use a smartcard connected to your local
machine on a remote server as if it were physically connected there.
Useful for SSH authentication, git signing, and other smartcard operations.

Features:
- gRPC-based protocol for efficient APDU forwarding
- TLS/mTLS for secure communication
- Automatic reconnection with exponential backoff
- vpcd integration for virtual smartcard readers

#---------------------------------------------------------------------------
# Server subpackage
#---------------------------------------------------------------------------
%package server
Summary:        Remote Smartcard Server
Requires:       pcsc-lite
Requires:       pcsc-lite-ccid
Requires:       opensc

# vpcd is in EPEL for RHEL/Rocky, or vsmartcard-vpcd in Fedora
%if 0%{?fedora}
Recommends:     vsmartcard-vpcd
%else
# For RHEL/Rocky - user needs to install from source or EPEL
Suggests:       vpcd
%endif

%{?systemd_requires}

%description server
Server component for Remote Smartcard. Install on the machine where
applications need to access the smartcard.

This component creates virtual smartcard readers using vpcd that applications
can use to communicate with remote smartcards.

Supported distributions:
- Fedora 38+
- RHEL 9+
- Rocky Linux 9+
- AlmaLinux 9+

#---------------------------------------------------------------------------
# Client subpackage
#---------------------------------------------------------------------------
%package client
Summary:        Remote Smartcard Client
Requires:       pcsc-lite
Requires:       pcsc-lite-ccid
Requires:       opensc
%{?systemd_requires}

%description client
Client component for Remote Smartcard. Install on the machine where
the physical smartcard is connected.

This component connects to the rsc-server and forwards smartcard commands
over a secure TLS connection.

Supported distributions:
- Fedora 38+
- RHEL 9+
- Rocky Linux 9+
- AlmaLinux 9+

#---------------------------------------------------------------------------
# Tools subpackage
#---------------------------------------------------------------------------
%package tools
Summary:        Remote Smartcard Tools
Requires:       openssl

%description tools
Certificate generation and testing tools for Remote Smartcard.

Includes:
- rsc-keygen: Generate CA, server, and client certificates
- rsc-generate-client-cert: Shell script for client cert generation

#---------------------------------------------------------------------------
# Build
#---------------------------------------------------------------------------
%prep
%autosetup -n %{name}-%{version}

%build
# Set Rust flags for release build
export CARGO_PROFILE_RELEASE_LTO=true
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1

cargo build --release --workspace

%install
# Server binary and service
install -D -m 755 target/release/rsc-server %{buildroot}%{_bindir}/rsc-server
install -D -m 644 systemd/rsc-server.service %{buildroot}%{_unitdir}/rsc-server.service

# Server config directories
install -d -m 755 %{buildroot}%{_sysconfdir}/rsc-server
install -d -m 700 %{buildroot}%{_sysconfdir}/rsc-server/certs

# Client binary and service
install -D -m 755 target/release/rsc-client %{buildroot}%{_bindir}/rsc-client
install -D -m 644 systemd/rsc-client.service %{buildroot}%{_unitdir}/rsc-client.service

# Client config directories
install -d -m 755 %{buildroot}%{_sysconfdir}/rsc-client
install -d -m 700 %{buildroot}%{_sysconfdir}/rsc-client/certs

# Tools
install -D -m 755 target/release/rsc-keygen %{buildroot}%{_bindir}/rsc-keygen
install -D -m 755 scripts/generate-client-cert.sh %{buildroot}%{_bindir}/rsc-generate-client-cert

# Documentation
install -d -m 755 %{buildroot}%{_docdir}/%{name}
install -m 644 README.md %{buildroot}%{_docdir}/%{name}/
install -m 644 CHANGELOG.md %{buildroot}%{_docdir}/%{name}/
install -m 644 SETUP.md %{buildroot}%{_docdir}/%{name}/
install -m 644 docs/*.md %{buildroot}%{_docdir}/%{name}/

#---------------------------------------------------------------------------
# Server scriptlets
#---------------------------------------------------------------------------
%pre server
# Create system user if not exists
getent group rsc-server >/dev/null || groupadd -r rsc-server
getent passwd rsc-server >/dev/null || \
    useradd -r -g rsc-server -s /sbin/nologin \
    -d %{_sysconfdir}/rsc-server -c "Remote Smartcard Server" rsc-server
exit 0

%post server
%systemd_post rsc-server.service
# Set ownership of config directory
chown -R rsc-server:rsc-server %{_sysconfdir}/rsc-server 2>/dev/null || true

%preun server
%systemd_preun rsc-server.service

%postun server
%systemd_postun_with_restart rsc-server.service
# Don't remove user on uninstall (may have data)

#---------------------------------------------------------------------------
# Client scriptlets
#---------------------------------------------------------------------------
%pre client
# Create system user if not exists
getent group rsc-client >/dev/null || groupadd -r rsc-client
getent passwd rsc-client >/dev/null || \
    useradd -r -g rsc-client -s /sbin/nologin \
    -d %{_sysconfdir}/rsc-client -c "Remote Smartcard Client" rsc-client
# Add to scard group for PC/SC access (if group exists)
usermod -a -G scard rsc-client 2>/dev/null || true
exit 0

%post client
%systemd_post rsc-client.service
# Set ownership of config directory
chown -R rsc-client:rsc-client %{_sysconfdir}/rsc-client 2>/dev/null || true

%preun client
%systemd_preun rsc-client.service

%postun client
%systemd_postun_with_restart rsc-client.service

#---------------------------------------------------------------------------
# Files
#---------------------------------------------------------------------------
%files server
%license LICENSE-MIT
%doc %{_docdir}/%{name}/
%{_bindir}/rsc-server
%{_unitdir}/rsc-server.service
%dir %attr(755,rsc-server,rsc-server) %{_sysconfdir}/rsc-server
%dir %attr(700,rsc-server,rsc-server) %{_sysconfdir}/rsc-server/certs

%files client
%license LICENSE-MIT
%doc %{_docdir}/%{name}/
%{_bindir}/rsc-client
%{_unitdir}/rsc-client.service
%dir %attr(755,rsc-client,rsc-client) %{_sysconfdir}/rsc-client
%dir %attr(700,rsc-client,rsc-client) %{_sysconfdir}/rsc-client/certs

%files tools
%license LICENSE-MIT
%{_bindir}/rsc-keygen
%{_bindir}/rsc-generate-client-cert

#---------------------------------------------------------------------------
# Changelog
#---------------------------------------------------------------------------
%changelog
* Fri Jan 24 2026 Remote Smartcard Contributors <noreply@example.com> - 0.1.0-1
- Initial release
- gRPC-based APDU forwarding with bidirectional streaming
- TLS/mTLS security with certificate-based authentication
- Automatic reconnection with exponential backoff
- Heartbeat mechanism for connection health monitoring
- vpcd integration for virtual smartcard readers
- Systemd service integration
- Support for Fedora 38+, RHEL 9+, Rocky Linux 9+
