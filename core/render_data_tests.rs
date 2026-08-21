use super::RenderData;

#[test]
fn dirty_signal_is_consumed_once() {
    let mut data = RenderData::new(64).unwrap();

    assert!(data.rows("signals").is_some());
    assert!(data.begin_render());
    assert!(!data.begin_render());

    data.mark_dirty();
    assert!(data.begin_render());
    assert!(!data.render_pending());
}

#[test]
fn skip_and_bundle_signals_hold_dirty_work() {
    let mut data = RenderData::new(64).unwrap();
    assert!(data.begin_render());

    data.write_signal(4, 1.0);
    data.mark_dirty();
    assert!(!data.begin_render());
    data.write_signal(4, 0.0);
    assert!(data.begin_render());

    data.write_signal(6, 1.0);
    data.mark_dirty();
    assert!(!data.begin_render());
    data.loadout_ready();
    assert!(data.begin_render());
}
