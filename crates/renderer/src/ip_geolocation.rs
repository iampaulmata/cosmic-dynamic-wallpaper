//! IP-geolocation location resolution (spec 7 US3) — a bundled offline `.mmdb`
//! database (research.md R3) plus STUN-based public-IP discovery (research.md R4, the
//! one external touchpoint an offline database can't supply on its own — disclosed to
//! the user before opt-in, FR-014; see `wallpaperctl`'s `IP_GEOLOCATION_DISCLOSURE`).
//!
//! **Concurrency note, a real and deliberate exception to this project's own
//! established posture**: every other async/blocking I/O in this daemon (the D-Bus
//! service, the portal subscription, `portal_location.rs`) runs inside `wallpaperd`'s
//! single `calloop` event loop via the `async-io`-backed `zbus`/`ashpd` stack — no
//! second concurrency model. `stunclient`'s only usable API here is genuinely
//! synchronous (`std::net::UdpSocket`-blocking); its `async` feature would require
//! `tokio`, which this project has deliberately kept out of `wallpaperd` throughout
//! (research.md R3/R5 of spec 6). Rather than force a blocking call onto the main loop
//! (stalling every output's scheduling for up to [`STUN_TIMEOUT`]) or pull in a second
//! async runtime, [`spawn`] runs on its own dedicated background OS thread — the one
//! place in this daemon that does, called out explicitly rather than silently
//! introduced.

use std::net::{IpAddr, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use maxminddb::geoip2;
use stunclient::StunClient;

use schedule_engine::Location;

use crate::config::{LocationConfigEntry, ResolutionStatus};

/// Public STUN server used for public-IP discovery (research.md R4) — the same
/// well-known Google STUN server `stunclient`'s own convenience helper defaults to.
pub const STUN_SERVER: &str = "stun.l.google.com:19302";

/// Public-IP discovery timeout — same posture as spec 6 research.md R6's portal
/// resolution timeout: generous enough for a real round trip, bounded so an
/// unreachable/blackholed STUN server can't stall resolution indefinitely.
pub const STUN_TIMEOUT: Duration = Duration::from_secs(5);

/// Public-IP cache TTL (research.md R4) — this external touchpoint happens at most a
/// few times a day, not per solar-event resolution.
pub const PUBLIC_IP_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Exponential backoff bounds after a failed resolution attempt — same shape as
/// `portal_location.rs`'s (spec 6 research.md R6): never a tight loop, self-recovers
/// without the user needing to manually toggle the mode off and on.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(30);
/// The backoff ceiling — never waited longer than this between retries.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// The next backoff delay after a failed attempt — doubles, capped at [`MAX_BACKOFF`].
pub fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_BACKOFF)
}

/// The well-known system path the bundled `.mmdb` database is installed to — a
/// release-process download (README.md's "IP-geolocation database" section), never
/// committed to this repository or built at compile/test time. Not present at all in a
/// dev/`cargo run` environment or this project's own CI — handled gracefully (a
/// resolution failure, not a crash), same posture as `starter_pack.rs`'s system path.
pub const MMDB_SYSTEM_PATH: &str = "/usr/share/dynamic-wallpaper/geoip.mmdb";

/// In-memory-only cache of the last STUN-discovered public IP (data-model.md) — never
/// written to `cosmic-config`; only the subsequent database lookup's *result*
/// (`ip_location`/`ip_status`) is persisted.
#[derive(Debug, Clone, Copy)]
pub struct PublicIpCache {
    /// The last STUN-discovered public IP address.
    pub address: IpAddr,
    /// When it was discovered — the cache is fresh until [`PUBLIC_IP_CACHE_TTL`] after
    /// this instant.
    pub resolved_at: Instant,
}

impl PublicIpCache {
    /// Whether this cached value is still within [`PUBLIC_IP_CACHE_TTL`] of `now`.
    pub fn is_fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.resolved_at) < PUBLIC_IP_CACHE_TTL
    }
}

/// A resolution outcome, sent from [`spawn`]'s background thread back to
/// `wallpaperd.rs`'s event loop over a `calloop::channel` — mirrors
/// `portal_location::PortalEvent`'s exact write-back contract (this daemon owns the
/// `Config` handle and in-memory state; the background thread never touches either
/// directly).
#[derive(Debug, Clone)]
pub enum IpGeoEvent {
    /// A successful resolution.
    Reading(Location),
    /// Any resolution failure (STUN discovery or the `.mmdb` lookup), with the
    /// specific reason.
    Failure(String),
}

/// Discover this machine's own public IP address via STUN (research.md R4) —
/// blocking. Callers on `wallpaperd`'s single `calloop` thread MUST run this on a
/// dedicated background thread (see this module's doc comment and [`spawn`]), never
/// call it directly from the event loop.
pub fn discover_public_ip_blocking() -> Result<IpAddr, String> {
    let resolved: Vec<std::net::SocketAddr> = STUN_SERVER
        .to_socket_addrs()
        .map_err(|e| format!("public IP discovery failed: STUN server DNS resolution failed: {e}"))?
        .collect();
    // Prefer an IPv4 result — this project's own live DNS resolution for the default
    // STUN server returns an IPv6 address *first* (a real, live-observed ordering
    // issue, not a hypothetical one), which would silently fail against an IPv4-only
    // wildcard bind below with an opaque "UDP socket error" (an address-family
    // mismatch, not a network problem) if picked naively via `.next()`.
    let server = resolved
        .iter()
        .find(|addr| addr.is_ipv4())
        .or_else(|| resolved.first())
        .copied()
        .ok_or_else(|| "public IP discovery failed: STUN server DNS resolution returned no addresses".to_string())?;
    let bind_addr = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let udp = UdpSocket::bind(bind_addr).map_err(|e| format!("public IP discovery failed: failed to bind UDP socket: {e}"))?;
    udp.set_read_timeout(Some(STUN_TIMEOUT)).map_err(|e| format!("public IP discovery failed: {e}"))?;
    let mut client = StunClient::new(server);
    client.set_timeout(STUN_TIMEOUT);
    // `stunclient::Error`'s `Display` is a fixed, generic phrase per variant (e.g.
    // "UDP socket error") — `{e:?}` (Debug) additionally surfaces the wrapped
    // `std::io::Error`'s real message, so a failure reason is actually specific, not a
    // generic catch-all (data-model.md's own requirement for every `ResolutionStatus::
    // Unavailable` reason).
    let addr = client.query_external_address(&udp).map_err(|e| format!("public IP discovery failed: {e} ({e:?})"))?;
    Ok(addr.ip())
}

/// Look up `ip`'s approximate location in the `.mmdb` database at `mmdb_path` —
/// fully local/offline, no network. Validates through spec 1's [`Location::new`]
/// before ever returning `Ok` (data-model.md's validate-before-write rule) — a
/// malformed or missing entry is a resolution failure, never a panic or a partial
/// write.
pub fn lookup_location(mmdb_path: &Path, ip: IpAddr) -> Result<Location, String> {
    let reader = maxminddb::Reader::open_readfile(mmdb_path).map_err(|e| format!("IP-geolocation database unavailable: {e}"))?;
    let record: geoip2::City = reader
        .lookup(ip)
        .map_err(|e| format!("IP-geolocation lookup failed: {e}"))?
        .decode()
        .map_err(|e| format!("IP-geolocation lookup failed: {e}"))?
        .ok_or_else(|| "IP-geolocation lookup failed: no location known for this IP".to_string())?;
    let latitude = record.location.latitude.ok_or_else(|| "IP-geolocation database has no latitude for this IP".to_string())?;
    let longitude = record.location.longitude.ok_or_else(|| "IP-geolocation database has no longitude for this IP".to_string())?;
    Location::new(latitude, longitude).map_err(|e| format!("IP-geolocation database returned an invalid location: {e}"))
}

/// Validate `location` and record a successful resolution — mirrors
/// `portal_location::apply_reading`'s exact posture for the `ip_*` fields instead of
/// `automatic_*`.
pub fn apply_reading(entry: &mut LocationConfigEntry, location: Location) {
    entry.ip_location = Some(location);
    entry.ip_status = ResolutionStatus::Resolved;
}

/// Record a resolution failure, written back immediately with no grace period —
/// mirrors `portal_location::apply_failure`'s exact posture (`ip_location` cleared,
/// not left stale, so `effective_location()`'s fallback to the manual value actually
/// triggers).
pub fn apply_failure(entry: &mut LocationConfigEntry, reason: String) {
    entry.ip_location = None;
    entry.ip_status = ResolutionStatus::Unavailable { reason };
}

/// Drive IP-geolocation resolution for the remainder of this daemon's lifetime (same
/// "spawned once, not cancelled on mode toggle" simplification `portal_location.rs`
/// documents) — on its own dedicated background thread (this module's doc comment),
/// not `wallpaperd`'s `calloop` loop. Respects [`PUBLIC_IP_CACHE_TTL`]: a fresh STUN
/// result is reused across resolution attempts rather than re-queried; only the
/// `.mmdb` lookup (fast, local) re-runs every attempt. Every outcome is sent to
/// `events`; returns only when `events` is disconnected (the daemon is shutting down).
pub fn spawn(mmdb_path: PathBuf, events: calloop::channel::Sender<IpGeoEvent>) {
    std::thread::spawn(move || {
        let mut cache: Option<PublicIpCache> = None;
        let mut backoff = INITIAL_BACKOFF;

        loop {
            let ip = match cache.filter(|c| c.is_fresh(Instant::now())) {
                Some(c) => Ok(c.address),
                None => discover_public_ip_blocking(),
            };

            let outcome = match ip {
                Ok(address) => {
                    cache = Some(PublicIpCache { address, resolved_at: Instant::now() });
                    lookup_location(&mmdb_path, address)
                }
                Err(reason) => Err(reason),
            };

            let (event, sleep_for) = match outcome {
                Ok(location) => {
                    backoff = INITIAL_BACKOFF;
                    (IpGeoEvent::Reading(location), PUBLIC_IP_CACHE_TTL)
                }
                Err(reason) => {
                    let wait = backoff;
                    backoff = next_backoff(backoff);
                    (IpGeoEvent::Failure(reason), wait)
                }
            };

            if events.send(event).is_err() {
                return; // The daemon is shutting down — nothing left to report to.
            }
            std::thread::sleep(sleep_for);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipnet::IpNet;
    use mmdb_writer::Writer;
    use serde::Serialize;

    #[derive(Serialize)]
    struct FixtureRecord {
        location: FixtureLocation,
    }
    #[derive(Serialize)]
    struct FixtureLocation {
        latitude: f64,
        longitude: f64,
    }

    /// Builds a tiny, in-memory `.mmdb` at test time (no binary fixture committed to
    /// the repo) mapping a handful of known test IPs to known coordinates, writes it
    /// to a tempfile, and returns its path.
    fn fixture_mmdb(entries: &[(&str, f64, f64)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let mut writer = Writer::new("Test-City");
        for (cidr, lat, lon) in entries {
            let network: IpNet = cidr.parse().unwrap();
            writer.insert(network, &FixtureRecord { location: FixtureLocation { latitude: *lat, longitude: *lon } }).unwrap();
        }
        let bytes = writer.to_bytes().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.mmdb");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    /// T046: `maxminddb` lookup against a small fixture `.mmdb` resolves known test
    /// IPs to expected coordinates, fully offline — no real STUN/network call.
    #[test]
    fn lookup_resolves_known_test_ips_to_expected_coordinates() {
        let (_dir, path) =
            fixture_mmdb(&[("203.0.113.0/24", 45.5019, -73.5674), ("198.51.100.0/24", 51.5072, -0.1276)]);

        let montreal = lookup_location(&path, "203.0.113.42".parse().unwrap()).unwrap();
        assert_eq!((montreal.latitude(), montreal.longitude()), (45.5019, -73.5674));

        let london = lookup_location(&path, "198.51.100.7".parse().unwrap()).unwrap();
        assert_eq!((london.latitude(), london.longitude()), (51.5072, -0.1276));
    }

    /// An IP with no covering network in the database is a clean resolution failure,
    /// not a panic.
    #[test]
    fn lookup_of_an_unknown_ip_is_a_clean_failure() {
        let (_dir, path) = fixture_mmdb(&[("203.0.113.0/24", 45.5019, -73.5674)]);
        let result = lookup_location(&path, "192.0.2.1".parse().unwrap());
        assert!(result.is_err());
    }

    /// A missing database file (this dev environment's own actual state — the real
    /// bundled `.mmdb` is a release-process download, never present here) is a clean
    /// resolution failure, not a panic.
    #[test]
    fn missing_database_file_is_a_clean_failure() {
        let result = lookup_location(Path::new("/nonexistent/geoip.mmdb"), "203.0.113.1".parse().unwrap());
        assert!(result.is_err());
    }

    /// T047: a lookup failure maps to `ResolutionStatus::Unavailable` with a specific
    /// reason string, never panics — exercised through `apply_failure`, the same
    /// write-back path a real STUN timeout or database-missing error takes.
    #[test]
    fn apply_failure_preserves_the_reason_verbatim() {
        let mut entry = LocationConfigEntry::default();
        apply_failure(&mut entry, "public IP discovery failed: STUN request timed out".to_string());
        assert_eq!(
            entry.ip_status,
            ResolutionStatus::Unavailable { reason: "public IP discovery failed: STUN request timed out".to_string() }
        );
        assert_eq!(entry.ip_location, None);
    }

    #[test]
    fn apply_reading_resolves_and_clears_any_prior_failure() {
        let mut entry = LocationConfigEntry::default();
        apply_failure(&mut entry, "public IP discovery failed: STUN request timed out".to_string());

        let (_dir, path) = fixture_mmdb(&[("203.0.113.0/24", 45.5019, -73.5674)]);
        let location = lookup_location(&path, "203.0.113.1".parse().unwrap()).unwrap();
        apply_reading(&mut entry, location);

        assert_eq!(entry.ip_status, ResolutionStatus::Resolved);
        assert_eq!(entry.ip_location, Some(location));
    }

    /// T048: the public-IP cache respects its 24-hour TTL — fresh just under the
    /// boundary, stale just at/over it.
    #[test]
    fn public_ip_cache_respects_its_ttl() {
        let now = Instant::now();
        let cache = PublicIpCache { address: "203.0.113.1".parse().unwrap(), resolved_at: now };

        assert!(cache.is_fresh(now));
        assert!(cache.is_fresh(now + PUBLIC_IP_CACHE_TTL - Duration::from_secs(1)));
        assert!(!cache.is_fresh(now + PUBLIC_IP_CACHE_TTL));
        assert!(!cache.is_fresh(now + PUBLIC_IP_CACHE_TTL + Duration::from_secs(1)));
    }

    /// Backoff doubles and caps, same contract as `portal_location.rs`'s identical
    /// bound — never a tight loop.
    #[test]
    fn next_backoff_doubles_and_caps() {
        let mut backoff = INITIAL_BACKOFF;
        for _ in 0..10 {
            backoff = next_backoff(backoff);
        }
        assert_eq!(backoff, MAX_BACKOFF);
    }
}
