use bitflags::bitflags;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct ParseFlags: u32 {
        const NONE = 0;
        const YIELD = 1 << 0;
        const AWAIT = 1 << 1;
        const TYPE = 1 << 2;
        const IGNORE_MISSING_OPEN_BRACE = 1 << 4;
        const JSDOC = 1 << 5;
    }
}
