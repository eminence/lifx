#![no_main]
use libfuzzer_sys::fuzz_target;

use lifx_core::BuildOptions;
use lifx_core::Message;
use lifx_core::RawMessage;

fn assert_message_eq(left: &Message, right: &Message) {
    match (left, right) {
        (
            Message::LightSetPower { level, duration },
            Message::LightSetPower {
                level: level2,
                duration: duration2,
            },
        ) => {
            assert_eq!(duration, duration2);
            if *level > 0 {
                assert!(*level2 > 0);
            } else {
                assert!(*level2 == 0);
            }
        }
        (a, b) => assert_eq!(a, b),
    }
}

fuzz_target!(|data: Message| {
    // build a raw message from this message
    let mut opts = BuildOptions {
        ..Default::default()
    };

    if let Message::Acknowledgement { seq } = data {
        opts.sequence = seq;
    }

    let orig = data.clone();
    let raw = RawMessage::build(&opts, data).unwrap();

    let parsed_msg = Message::from_raw(&raw).unwrap();
    assert_message_eq(&orig, &parsed_msg);
});
