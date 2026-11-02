# Local change to ksni 0.3.7

Source: https://crates.io/crates/ksni/0.3.7 (upstream revision is recorded in
`.cargo_vcs_info.json`). The upstream Unlicense is retained in `UNLICENSE`.

This copy adds `Tray::label()` and `Tray::label_guide()`, the corresponding
`XAyatanaLabel` and `XAyatanaLabelGuide` properties, and `XAyatanaNewLabel` plus
property-change notifications. No registration, menu or activation behavior is
changed. These properties let a host render an icon and text inside one item.

Protocol reference:
https://github.com/AyatanaIndicators/libayatana-appindicator/blob/master/src/notification-item.xml

The label and update notifications are exercised against the real Rust service
in `tools/checks/src/desktop/tray.rs`. Remove this local copy when upstream provides the same
extension, keeping that integration test.
