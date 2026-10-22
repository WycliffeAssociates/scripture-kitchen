//! Every finding code, and the check each one belongs to.
//!
//! ```text
//! hygiene.C0Control      -> None              no setting tunes it
//! presence.Missing       -> Some(Presence)    one check for the three kinds
//! convention.Casing      -> Some(Casing)      one check per channel
//! sourceCopy             -> Some(SourceCopy)
//! ```

use crate::codec::{HygieneClass, PresenceKind};
use crate::judge::Channel;

/// What a settings page groups by: one per convention channel, one per
/// source-compared lane. One level coarser than a finding code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Convention(Channel),
    Presence,
    SourceCopy,
    LengthProportionality,
}

impl Check {
    pub const ALL: [Self; 14] = [
        Self::Convention(Channel::ExactNeighbor),
        Self::Convention(Channel::PooledNeighbor),
        Self::Convention(Channel::RunShape),
        Self::Convention(Channel::Placement),
        Self::Convention(Channel::Rarity),
        Self::Convention(Channel::Casing),
        Self::Convention(Channel::WordLength),
        Self::Convention(Channel::Doubled),
        Self::Convention(Channel::LetterRun),
        Self::Convention(Channel::SentenceStart),
        Self::Convention(Channel::BookRate),
        Self::Presence,
        Self::SourceCopy,
        Self::LengthProportionality,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Convention(channel) => channel.name(),
            Self::Presence => "Presence",
            Self::SourceCopy => "SourceCopy",
            Self::LengthProportionality => "LengthProportionality",
        }
    }
}

/// One finding code as `SousCode` spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingCode {
    pub code: String,
    /// What the code finds, in one plain sentence.
    pub rule: &'static str,
    /// `None` for hygiene, which no setting tunes.
    pub check: Option<Check>,
}

/// Every finding code in wire order: hygiene classes, presence kinds,
/// convention channels, then the two lanes with no table.
pub fn finding_codes() -> Vec<FindingCode> {
    let code = |code: String, rule, check| FindingCode { code, rule, check };
    let hygiene = HygieneClass::ALL
        .iter()
        .map(|class| code(format!("hygiene.{}", class.name()), class.rule(), None));
    let presence = PresenceKind::ALL.iter().map(|kind| {
        code(
            format!("presence.{}", kind.name()),
            kind.rule(),
            Some(Check::Presence),
        )
    });
    let convention = Channel::ALL.iter().map(|channel| {
        code(
            format!("convention.{}", channel.name()),
            channel.rule(),
            Some(Check::Convention(*channel)),
        )
    });
    let lanes = [
        code(
            "sourceCopy".to_string(),
            "Words in a row spelled exactly as in the same verse of the source text, as if pasted in.",
            Some(Check::SourceCopy),
        ),
        code(
            "lengthProportionality".to_string(),
            "A verse far longer or shorter, against its source verse, than this project usually is.",
            Some(Check::LengthProportionality),
        ),
    ];
    hygiene
        .chain(presence)
        .chain(convention)
        .chain(lanes)
        .collect()
}
