Name:           remote-smartcard
Version:        0.1.0
Release:        1%{?dist}
Summary:        Remote Smartcard - Use local smartcards on remote servers

License:        MIT
URL:            https://github.com/alun-hub/remote-smartcard
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  protobuf-compiler
BuildRequires:  pcsc-lite-devel
BuildRequires:  systemd-rpm-macros

%description
Remote Smartcard allows you to use a smartcard connected to your local
machine on a remote server as if it were physically connected there.
Useful for SSH authentication, git signing, and other smartcard operations.

%package server
Summary:        Remote Smartcard Server
Requires:       pcsc-lite
Requires:       opensc
Recommends:     vsmartcard-vpcd
%{?systemd_requires}

%description server
Server component for Remote Smartcard. Install on the machine where
applications need to access the smartcard.

%package client
Summary:        Remote Smartcard Client
Requires:       pcsc-lite
Requires:       opensc
%{?systemd_requires}

%description client
Client component for Remote Smartcard. Install on the machine where
the physical smartcard is connected.

%package tools
Summary:        Remote Smartcard Tools
Requires:       openssl

%description tools
Certificate generation and testing tools for Remote Smartcard.

%prep
%autosetup

%build
cargo build --release --workspace

%install
# Server
install -D -m 755 target/release/rsc-server %{buildroot}%{_bindir}/rsc-server
install -D -m 644 systemd/rsc-server.service %{buildroot}%{_unitdir}/rsc-server.service
install -d -m 755 %{buildroot}%{_sysconfdir}/rsc-server/certs

# Client
install -D -m 755 target/release/rsc-client %{buildroot}%{_bindir}/rsc-client
install -D -m 644 systemd/rsc-client.service %{buildroot}%{_unitdir}/rsc-client.service
install -d -m 755 %{buildroot}%{_sysconfdir}/rsc-client/certs

# Tools
install -D -m 755 target/release/rsc-keygen %{buildroot}%{_bindir}/rsc-keygen
install -D -m 755 scripts/generate-client-cert.sh %{buildroot}%{_bindir}/rsc-generate-client-cert

%pre server
getent passwd rsc-server >/dev/null || \
    useradd -r -s /sbin/nologin -d / rsc-server

%pre client
getent passwd rsc-client >/dev/null || \
    useradd -r -s /sbin/nologin -d / rsc-client
usermod -a -G scard rsc-client 2>/dev/null || true

%post server
%systemd_post rsc-server.service
chown -R rsc-server:rsc-server %{_sysconfdir}/rsc-server
chmod 700 %{_sysconfdir}/rsc-server/certs

%post client
%systemd_post rsc-client.service
chown -R rsc-client:rsc-client %{_sysconfdir}/rsc-client
chmod 700 %{_sysconfdir}/rsc-client/certs

%preun server
%systemd_preun rsc-server.service

%preun client
%systemd_preun rsc-client.service

%postun server
%systemd_postun_with_restart rsc-server.service

%postun client
%systemd_postun_with_restart rsc-client.service

%files server
%license LICENSE-MIT
%doc README.md docs/
%{_bindir}/rsc-server
%{_unitdir}/rsc-server.service
%dir %{_sysconfdir}/rsc-server
%dir %{_sysconfdir}/rsc-server/certs

%files client
%license LICENSE-MIT
%doc README.md docs/
%{_bindir}/rsc-client
%{_unitdir}/rsc-client.service
%dir %{_sysconfdir}/rsc-client
%dir %{_sysconfdir}/rsc-client/certs

%files tools
%license LICENSE-MIT
%{_bindir}/rsc-keygen
%{_bindir}/rsc-generate-client-cert

%changelog
* Fri Jan 24 2026 Remote Smartcard Contributors <noreply@example.com> - 0.1.0-1
- Initial release
- gRPC-based APDU forwarding
- mTLS security
- Automatic reconnection with exponential backoff
- vpcd integration for virtual smartcard readers
