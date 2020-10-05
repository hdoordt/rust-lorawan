#![allow(dead_code)]

const UPLINK_CHANNEL_MAP: [u32; 6] = [
    864_100_00, 864_300_00, 864_500_00, 868_100_00, 868_300_00, 868_500_00,
];

const DOWNLINK_CHANNEL_MAP: [u32; 3] = [868_100_00, 868_300_00, 868_500_00];

const RECEIVE_DELAY1: usize = 1;
const RECEIVE_DELAY2: usize = RECEIVE_DELAY1 + 1; // must be RECEIVE_DELAY + 1 s
const JOIN_ACCEPT_DELAY1: usize = 5;
const JOIN_ACCEPT_DELAY2: usize = 6;
const MAX_FCNT_GAP: usize = 16384;
const ADR_ACK_LIMIT: usize = 64;
const ADR_ACK_DELAY: usize = 32;
const ACK_TIMEOUT: usize = 2; // random delay between 1 and 3 seconds

pub struct Configuration {
    subband: Option<u8>,
    last_join: u8,
}
impl Configuration {
    pub fn new() -> Configuration {
        Configuration {
            subband: None,
            last_join: 0,
        }
    }

    pub fn set_subband(&mut self, subband: u8) {
        self.subband = Some(subband);
    }

    pub fn get_join_frequency(&mut self, random: u8) -> u32 {
        // let subband = if let Some(subband) = &self.subband {
        //     subband - 1
        // } else {
        //     (random >> 3) & 0b111
        // };
        let subband = 0;
        self.last_join = subband;
        UPLINK_CHANNEL_MAP[subband as usize]
    }

    pub fn get_data_frequency(&mut self, random: u8) -> u32 {
        let subband = if let Some(subband) = &self.subband {
            subband - 1
        } else {
            (random >> 3) & 0b111
        };
        UPLINK_CHANNEL_MAP[subband as usize]
    }

    pub fn get_join_accept_frequency1(&mut self) -> u32 {
        DOWNLINK_CHANNEL_MAP[self.last_join as usize]
    }

    pub fn get_join_accept_delay1(&mut self) -> usize {
        JOIN_ACCEPT_DELAY1
    }

    pub fn get_join_accept_delay2(&mut self) -> usize {
        JOIN_ACCEPT_DELAY2
    }
}
