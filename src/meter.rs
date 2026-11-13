//! Time-signature helpers. The audio thread only sees the `[u8; BEATS_MAX]`
//! table from [`pattern_to_codes`]: 2 is a high click, 1 is a low click, and
//! 0 is silence.

pub const BEATS_MAX: u32 = 16;
pub const BEAT_UNIT_DEFAULT: u32 = 4;

pub const CODE_MUTE: u8 = 0;
pub const CODE_NORMAL: u8 = 1;
pub const CODE_ACCENT: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatAccent {
    Accent,
    Normal,
    Mute,
}

impl BeatAccent {
    pub fn from_char(c: char) -> Self {
        match c {
            'A' => Self::Accent,
            'm' => Self::Mute,
            _ => Self::Normal,
        }
    }

    pub fn to_char(self) -> char {
        match self {
            Self::Accent => 'A',
            Self::Normal => 'n',
            Self::Mute => 'm',
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Accent => Self::Normal,
            Self::Normal | Self::Mute => Self::Accent,
        }
    }
}

pub fn is_valid_unit(unit: u32) -> bool {
    matches!(unit, 2 | 4 | 8 | 16)
}

/// Keeps a pattern at exactly `beats` characters. Extra beats are normal,
/// and a missing pattern starts with a single accent.
pub fn fit_pattern(pattern: &str, beats: u32) -> String {
    let beats = beats.clamp(1, BEATS_MAX) as usize;
    let mut chars: Vec<char> = pattern
        .chars()
        .filter_map(|c| match c {
            'A' => Some('A'),
            'n' | 'm' => Some('n'),
            _ => None,
        })
        .collect();
    if chars.is_empty() {
        chars.push('A');
    }
    if chars.len() > beats {
        chars.truncate(beats);
    } else {
        chars.resize(beats, 'n');
    }
    chars.into_iter().collect()
}

pub fn default_pattern(beats: u32) -> String {
    fit_pattern("A", beats)
}

pub fn pattern_to_codes(pattern: &str) -> [u8; BEATS_MAX as usize] {
    let mut codes = [CODE_NORMAL; BEATS_MAX as usize];
    for (index, c) in pattern.chars().take(BEATS_MAX as usize).enumerate() {
        codes[index] = match BeatAccent::from_char(c) {
            BeatAccent::Accent => CODE_ACCENT,
            BeatAccent::Normal => CODE_NORMAL,
            BeatAccent::Mute => CODE_MUTE,
        };
    }
    codes
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct Grouping {
    pub label: &'static str,
    pub groups: &'static [u32],
}

#[cfg_attr(not(test), allow(dead_code))]
const GROUPINGS_5: &[Grouping] = &[
    Grouping {
        label: "3+2",
        groups: &[3, 2],
    },
    Grouping {
        label: "2+3",
        groups: &[2, 3],
    },
];

#[cfg_attr(not(test), allow(dead_code))]
const GROUPINGS_6: &[Grouping] = &[
    Grouping {
        label: "3+3",
        groups: &[3, 3],
    },
    Grouping {
        label: "2+2+2",
        groups: &[2, 2, 2],
    },
];

#[cfg_attr(not(test), allow(dead_code))]
const GROUPINGS_7: &[Grouping] = &[
    Grouping {
        label: "2+2+3",
        groups: &[2, 2, 3],
    },
    Grouping {
        label: "3+2+2",
        groups: &[3, 2, 2],
    },
    Grouping {
        label: "2+3+2",
        groups: &[2, 3, 2],
    },
];

#[cfg_attr(not(test), allow(dead_code))]
const GROUPINGS_8: &[Grouping] = &[
    Grouping {
        label: "3+3+2",
        groups: &[3, 3, 2],
    },
    Grouping {
        label: "3+2+3",
        groups: &[3, 2, 3],
    },
    Grouping {
        label: "2+3+3",
        groups: &[2, 3, 3],
    },
];

#[cfg_attr(not(test), allow(dead_code))]
const GROUPINGS_9: &[Grouping] = &[
    Grouping {
        label: "2+2+2+3",
        groups: &[2, 2, 2, 3],
    },
    Grouping {
        label: "3+2+2+2",
        groups: &[3, 2, 2, 2],
    },
    Grouping {
        label: "2+3+2+2",
        groups: &[2, 3, 2, 2],
    },
    Grouping {
        label: "2+2+3+2",
        groups: &[2, 2, 3, 2],
    },
];

#[cfg_attr(not(test), allow(dead_code))]
const GROUPINGS_12: &[Grouping] = &[Grouping {
    label: "3+3+3+3",
    groups: &[3, 3, 3, 3],
}];

#[cfg_attr(not(test), allow(dead_code))]
pub fn groupings_for(beats: u32) -> &'static [Grouping] {
    match beats {
        5 => GROUPINGS_5,
        6 => GROUPINGS_6,
        7 => GROUPINGS_7,
        8 => GROUPINGS_8,
        9 => GROUPINGS_9,
        12 => GROUPINGS_12,
        _ => &[],
    }
}

/// Accent on the first beat of each group, normal on the rest.
#[cfg_attr(not(test), allow(dead_code))]
pub fn pattern_from_groups(groups: &[u32]) -> String {
    let mut pattern = String::new();
    for size in groups {
        for beat in 0..*size {
            pattern.push(if beat == 0 { 'A' } else { 'n' });
        }
    }
    pattern
}

pub const SAVED_METERS_MAX: usize = 12;

/// A time signature the user can select, including its accent pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meter {
    pub beats: u32,
    pub unit: u32,
    pub pattern: String,
}

impl Meter {
    pub fn new(beats: u32, unit: u32, pattern: impl AsRef<str>) -> Self {
        let beats = beats.clamp(1, BEATS_MAX);
        let unit = if is_valid_unit(unit) {
            unit
        } else {
            BEAT_UNIT_DEFAULT
        };
        Self {
            beats,
            unit,
            pattern: fit_pattern(pattern.as_ref(), beats),
        }
    }

    pub fn encode(&self) -> String {
        format!("{}/{}/{}", self.beats, self.unit, self.pattern)
    }

    pub fn decode(value: &str) -> Option<Self> {
        let mut parts = value.splitn(3, '/');
        let beats: u32 = parts.next()?.parse().ok()?;
        let unit: u32 = parts.next()?.parse().ok()?;
        let pattern = parts.next()?;
        if !(1..=BEATS_MAX).contains(&beats) || !is_valid_unit(unit) {
            return None;
        }
        Some(Self::new(beats, unit, pattern))
    }

    pub fn same_signature(&self, other: &Self) -> bool {
        self.beats == other.beats && self.unit == other.unit
    }

    pub fn defaults() -> Vec<Self> {
        vec![
            Self::new(2, 4, "An"),
            Self::new(3, 4, "Ann"),
            Self::new(4, 4, "Annn"),
            Self::new(6, 8, "Annnnn"),
        ]
    }
}

/// Drops repeated signatures and orders the list by beat unit, then by the
/// number of beats. The active meter wins when two entries share a signature.
pub fn normalize_list(meters: &[Meter], active: &Meter) -> Vec<Meter> {
    let mut unique = Vec::with_capacity(meters.len() + 1);
    for meter in meters {
        if unique.iter().any(|saved: &Meter| saved.same_signature(meter)) {
            continue;
        }
        unique.push(meter.clone());
    }
    if let Some(saved) = unique.iter_mut().find(|saved| saved.same_signature(active)) {
        *saved = active.clone();
    } else {
        unique.push(active.clone());
    }
    unique.sort_by(|left, right| left.unit.cmp(&right.unit).then(left.beats.cmp(&right.beats)));
    unique
}

/// The next signature that is not already saved. Common meters come first.
#[cfg_attr(not(test), allow(dead_code))]
pub fn fresh_meter(existing: &[Meter]) -> Option<Meter> {
    if existing.len() >= SAVED_METERS_MAX {
        return None;
    }
    const PREFERRED: &[(u32, u32)] = &[
        (5, 4),
        (7, 8),
        (5, 8),
        (7, 4),
        (9, 8),
        (12, 8),
        (3, 8),
        (2, 2),
        (6, 4),
        (8, 8),
    ];
    let preferred = PREFERRED.iter().copied();
    let rest = (1..=BEATS_MAX).flat_map(|beats| [4, 8, 2, 16].into_iter().map(move |unit| (beats, unit)));
    preferred.chain(rest).find_map(|(beats, unit)| {
        let meter = Meter::new(beats, unit, "A");
        if existing.iter().any(|saved| saved.same_signature(&meter)) {
            None
        } else {
            Some(meter)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pattern_accents_only_the_first_beat() {
        assert_eq!(default_pattern(1), "A");
        assert_eq!(default_pattern(4), "Annn");
        assert_eq!(default_pattern(6), "Annnnn");
    }

    #[test]
    fn fit_pattern_appends_normal_beats_and_truncates() {
        assert_eq!(fit_pattern("AnnAn", 7), "AnnAnnn");
        assert_eq!(fit_pattern("AnnAn", 3), "Ann");
        assert_eq!(fit_pattern("m", 4), "nnnn");
        assert_eq!(fit_pattern("", 4), "Annn");
        assert_eq!(fit_pattern("Axyz", 3), "Ann");
    }

    #[test]
    fn groupings_match_the_common_meters() {
        let cases = [
            (5, "3+2", "AnnAn"),
            (5, "2+3", "AnAnn"),
            (6, "3+3", "AnnAnn"),
            (6, "2+2+2", "AnAnAn"),
            (7, "2+2+3", "AnAnAnn"),
            (7, "3+2+2", "AnnAnAn"),
            (7, "2+3+2", "AnAnnAn"),
            (8, "3+3+2", "AnnAnnAn"),
            (9, "2+2+2+3", "AnAnAnAnn"),
            (12, "3+3+3+3", "AnnAnnAnnAnn"),
        ];
        for (beats, label, expected) in cases {
            let grouping = groupings_for(beats)
                .iter()
                .find(|grouping| grouping.label == label)
                .unwrap();
            let sum: u32 = grouping.groups.iter().sum();
            assert_eq!(sum, beats);
            assert_eq!(pattern_from_groups(grouping.groups), expected);
        }
    }

    #[test]
    fn saved_meters_round_trip_and_skip_duplicates() {
        let encoded = Meter::new(7, 4, "AnAnAnn").encode();
        assert_eq!(encoded, "7/4/AnAnAnn");
        assert_eq!(Meter::decode(&encoded), Some(Meter::new(7, 4, "AnAnAnn")));
        assert!(Meter::decode("0/4/A").is_none());
        assert!(Meter::decode("4/3/Annn").is_none());

        let mut meters = Meter::defaults();
        let added = fresh_meter(&meters).unwrap();
        assert_eq!(added, Meter::new(5, 4, "Annnn"));
        meters.push(added);
        assert!(fresh_meter(&meters).unwrap() != Meter::new(5, 4, "Annnn"));
    }

    #[test]
    fn saved_meters_drop_duplicate_signatures_and_group_by_unit() {
        let active = Meter::new(4, 4, "Ammm");
        let meters = vec![
            Meter::new(6, 8, "Annnnn"),
            Meter::new(4, 4, "Annn"),
            Meter::new(3, 4, "Ann"),
            Meter::new(2, 2, "An"),
            active.clone(),
        ];
        let normalized = normalize_list(&meters, &active);
        assert_eq!(
            normalized
                .iter()
                .map(|meter| (meter.beats, meter.unit))
                .collect::<Vec<_>>(),
            vec![(2, 2), (3, 4), (4, 4), (6, 8)]
        );
        assert_eq!(normalized[2].pattern, "Annn");
    }

    #[test]
    fn pattern_codes_use_the_audio_thread_values() {
        let codes = pattern_to_codes("Anm");
        assert_eq!(codes[0], CODE_ACCENT);
        assert_eq!(codes[1], CODE_NORMAL);
        assert_eq!(codes[2], CODE_MUTE);
        assert_eq!(codes[3], CODE_NORMAL);
    }
}
