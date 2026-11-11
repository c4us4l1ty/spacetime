//! Honey-Duct: a fake C2 endpoint.
//!
//! When malware beacons home, a plain block tells it "the internet is broken",
//! which is a signal it can detect. The sinkhole instead answers convincingly:
//! TCP accept, a plausible HTTPS handshake, `200 OK` for HTTP, and canned replies
//! for the most common botnet command verbs. The operator then watches what the
//! implant tries to do next instead of losing the sample.

use std::collections::BTreeMap;

/// What the sinkhole pretends to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkholeProfile {
    /// Generic HTTP `200 OK` with an empty JSON body.
    HttpOk,
    /// Generic HTTPS (the switch terminates the TLS records; the guest sees a
    /// certificate the sinkhole minted for the destination domain).
    HttpsOk,
    /// "Command accepted, standing by" — the classic botnet C2 handshake.
    C2Standby,
    /// Slow drip: accept, then never answer (useful to waste an implant's time).
    Tarpit,
}

/// A recorded exchange with a trapped implant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    pub request: Vec<u8>,
    pub response: Vec<u8>,
    pub profile: SinkholeProfile,
}

/// The in-memory deception engine.
#[derive(Debug, Clone)]
pub struct Sinkhole {
    profile: SinkholeProfile,
    /// Bytes the guest sent, keyed by destination `ip:port`.
    captured: BTreeMap<String, Vec<u8>>,
    exchanges: Vec<Exchange>,
}

impl Default for Sinkhole {
    fn default() -> Self {
        Sinkhole {
            profile: SinkholeProfile::C2Standby,
            captured: BTreeMap::new(),
            exchanges: Vec::new(),
        }
    }
}

impl Sinkhole {
    pub fn new(profile: SinkholeProfile) -> Sinkhole {
        Sinkhole {
            profile,
            captured: BTreeMap::new(),
            exchanges: Vec::new(),
        }
    }

    pub fn profile(&self) -> SinkholeProfile { self.profile }

    pub fn set_profile(&mut self, profile: SinkholeProfile) {
        self.profile = profile;
    }

    /// Consume one guest request destined for `endpoint` and return what the
    /// implant should receive back.
    pub fn respond(&mut self, endpoint: &str, request: &[u8]) -> Vec<u8> {
        self.captured
            .entry(endpoint.to_string())
            .or_default()
            .extend_from_slice(request);

        let response: Vec<u8> = match self.profile {
            SinkholeProfile::HttpOk => {
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 27\r\n\r\n{\"status\":\"ok\",\"data\":[]}".to_vec()
            }
            SinkholeProfile::HttpsOk => {
                b"\x16\x03\x03\x00\x3dHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec()
            }
            SinkholeProfile::C2Standby => {
                let body = if request.windows(4).any(|w| w == b"PING") {
                    "PONG\n"
                } else if request.windows(3).any(|w| w == b"GET") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nCMD:SNORE"
                } else {
                    "OK\n"
                };
                body.as_bytes().to_vec()
            }
            SinkholeProfile::Tarpit => Vec::new(),
        };

        self.exchanges.push(Exchange {
            request: request.to_vec(),
            response: response.clone(),
            profile: self.profile,
        });
        response
    }

    /// Everything the implant has sent so far (the forensic payload).
    pub fn captured(&self) -> &BTreeMap<String, Vec<u8>> { &self.captured }

    pub fn exchanges(&self) -> &[Exchange] { &self.exchanges }

    pub fn captured_bytes(&self) -> u64 { self.captured.values().map(|v| v.len() as u64).sum() }

    /// Human summary of the captured traffic, e.g. for the console drawer.
    pub fn transcript(&self, endpoint: &str) -> Option<String> {
        let bytes = self.captured.get(endpoint)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c2_profile_answers_ping_with_pong() {
        let mut sink = Sinkhole::default();
        let reply = sink.respond("185.220.101.1:4444", b"PING");
        assert_eq(reply, b"PONG\n");        assert_eq(sink.captured_bytes(), 4);    }

    #[test]
    fn http_profile_returns_plausible_response() {
        let mut sink = Sinkhole::new(SinkholeProfile::HttpOk);
        let reply = sink.respond("1.2.3.4:80", b"GET / HTTP/1.1\r\n\r\n");
        let text = String::from_utf8(reply).unwrap();
        assert(text.starts_with("HTTP/1.1 200 OK"));        assert(text.contains("\"status\":\"ok\""));    }

    #[test]
    fn tarpit_records_but_never_speaks() {
        let mut sink = Sinkhole::new(SinkholeProfile::Tarpit);
        assert(sink.respond("evil:1", b"hello").is_empty());        assert_eq(sink.exchanges().len(), 1);    }

    #[test]
    fn transcript_is_human_readable() {
        let mut sink = Sinkhole::default();
        sink.respond("evil:1", b"GET /cmd?id=42");
        assert(sink.transcript("evil:1").unwrap().contains("cmd"));        assert(sink.transcript("nope:1").is_none());    }
}
