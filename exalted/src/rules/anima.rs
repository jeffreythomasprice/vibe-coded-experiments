use serde::{Deserialize, Serialize};

use crate::character::Caste;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimaPowerKind {
    Action,
    Reflexive,
    Touch,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimaPower {
    pub name: &'static str,
    pub cost: &'static str,
    pub kind: AnimaPowerKind,
    pub effect: &'static str,
    pub page: u16,
}

const DAWN: &[AnimaPower] = &[AnimaPower {
    name: "Terrifying Aspect of the Sun-King",
    cost: "10m",
    kind: AnimaPowerKind::Action,
    effect: "For the rest of the scene the Dawn appears huge and terrifying, with burning \
             eyes and a rending grasp. Any creature whose Valor is at most the Dawn's Essence \
             cannot look directly at her or strongly oppose her; the Dawn gains +2 to her DV \
             against affected characters. Mortals (and normal animals) must roll Valor or flee \
             in terror. No effect on golems, automata, the walking dead, or other creatures \
             that do not know fear.",
    page: 93,
}];

const ZENITH: &[AnimaPower] = &[
    AnimaPower {
        name: "Sun-Cleansing Pyre",
        cost: "1m/body",
        kind: AnimaPowerKind::Touch,
        effect: "Burns the body of a fallen creature to ash in (Essence) ticks, sending the \
                 soul to Heaven and preventing it from rising as a zombie or hungry ghost.",
        page: 94,
    },
    AnimaPower {
        name: "Holy Radiance of the Daystar",
        cost: "10m",
        kind: AnimaPowerKind::Action,
        effect: "The Zenith glows like noon for (Essence * 10) yards. For the rest of the \
                 scene (or until released) she gains additional lethal and bashing soak equal \
                 to Essence against attacks from creatures of darkness, and her player adds \
                 Essence to the minimum number of dice rolled on any attack against a creature \
                 of darkness.",
        page: 94,
    },
];

const TWILIGHT: &[AnimaPower] = &[AnimaPower {
    name: "Mantle of Brilliant Twilight",
    cost: "5m",
    kind: AnimaPowerKind::Reflexive,
    effect: "If a damage roll would cost the Twilight one or more health levels, she may \
             spend 5 motes to subtract 1 health level of damage per dot of her Essence from \
             that single attack. Can negate a weak attack entirely or greatly soften a deadly \
             one.",
    page: 96,
}];

const NIGHT: &[AnimaPower] = &[
    AnimaPower {
        name: "Anima Muting",
        cost: "+1m per non-Obvious Charm, or double the normal cost for an Obvious Charm",
        kind: AnimaPowerKind::Passive,
        effect: "When the Night spends Peripheral Essence on a Charm she may pay extra motes \
                 so the expenditure does not add to her anima banner. Muting does not work on \
                 sorcery.",
        page: 98,
    },
    AnimaPower {
        name: "Muted Veil of Night",
        cost: "10m",
        kind: AnimaPowerKind::Action,
        effect: "Extends an imperceptible muting veil for the scene. Senses (sight, smell, \
                 sound) and physical traces (shadows, footprints, scent) of the Night are \
                 dulled — anyone trying to notice or track her suffers a penalty equal to her \
                 Essence (round up). Once she spends 11-15 motes of Peripheral Essence she \
                 becomes as visible as any other Solar in full anima, but everything before \
                 that point remains hidden.",
        page: 98,
    },
];

const ECLIPSE: &[AnimaPower] = &[
    AnimaPower {
        name: "Sanctification of Oaths",
        cost: "10m + 1wp",
        kind: AnimaPowerKind::Action,
        effect: "The Eclipse seals an oath she witnesses or is party to with a handclasp; her \
                 anima blazes with the words and runes of Heaven's law. A number of times \
                 equal to the Eclipse's Essence at sanctification, any oathbreaker (including \
                 the Eclipse herself) will horribly botch a critical action at the worst \
                 possible moment. The Eclipse need not even be alive when the curse fires. \
                 Becomes invocable without mote cost once the Solar spends 11-15 motes of \
                 Peripheral Essence in the scene.",
        page: 100,
    },
    AnimaPower {
        name: "Diplomatic Protection",
        cost: "—",
        kind: AnimaPowerKind::Passive,
        effect: "Spirits, demon princes, Fair Folk, and similar beings who legitimately deal \
                 with a recognized Eclipse must obey the rules of hospitality. They may \
                 refuse business uncompelled, but cannot attack members of an embassy unless \
                 the Eclipse provokes them into breaking the peace.",
        page: 100,
    },
];

const UNIVERSAL: &[AnimaPower] = &[
    AnimaPower {
        name: "Caste Mark Glow",
        cost: "1m",
        kind: AnimaPowerKind::Reflexive,
        effect: "Cause the caste mark to glow brightly for a scene (as if 4-7m of Peripheral \
                 were burned).",
        page: 92,
    },
    AnimaPower {
        name: "Read-By Anima",
        cost: "1m",
        kind: AnimaPowerKind::Reflexive,
        effect: "Cause the anima to glow bright enough to read by for a scene (as if 8-10m \
                 of Peripheral were burned).",
        page: 92,
    },
    AnimaPower {
        name: "Time of Day",
        cost: "1m",
        kind: AnimaPowerKind::Reflexive,
        effect: "Know the precise time of day for the rest of the scene.",
        page: 92,
    },
];

pub fn powers_for(caste: Caste) -> &'static [AnimaPower] {
    match caste {
        Caste::Dawn => DAWN,
        Caste::Zenith => ZENITH,
        Caste::Twilight => TWILIGHT,
        Caste::Night => NIGHT,
        Caste::Eclipse => ECLIPSE,
    }
}

pub fn universal_powers() -> &'static [AnimaPower] {
    UNIVERSAL
}
