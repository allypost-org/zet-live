use crate::proto::gtfs_realtime::data::transit_realtime::trip_update::StopTimeUpdate;

/// An arrival this many seconds in the past still counts as upcoming.
const PASSED_GRACE_SECS: i64 = 30;

/// Selects the stop that the vehicle goes to next.
///
/// ZET feeds can keep stale updates with an old delay, so arrival times are
/// not always ascending by `stop_sequence`. An update with an arrival in the
/// past shows that the vehicle reached that stop, and thus all earlier stops.
/// The next stop is the first update after the last reached one.
///
/// Updates before `current_stop_sequence` are ignored. If all updates are
/// reached, the last update is returned.
pub fn select_next_stop(
    stop_time_updates: &[StopTimeUpdate],
    current_stop_sequence: Option<u32>,
    now_secs: i64,
) -> Option<&StopTimeUpdate> {
    let candidates = stop_time_updates
        .iter()
        .filter(|stu| {
            current_stop_sequence.is_none_or(|min| stu.stop_sequence.is_none_or(|seq| seq >= min))
        })
        .collect::<Vec<_>>();

    let first_unreached = candidates
        .iter()
        .rposition(|stu| {
            stu.arrival
                .as_ref()
                .and_then(|a| a.time)
                .is_some_and(|time| time < now_secs - PASSED_GRACE_SECS)
        })
        .map_or(0, |i| i + 1);

    candidates
        .get(first_unreached)
        .copied()
        .or_else(|| stop_time_updates.last())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::gtfs_realtime::data::transit_realtime::trip_update::StopTimeEvent;

    fn stu(stop_sequence: u32, arrival_time: i64) -> StopTimeUpdate {
        StopTimeUpdate {
            stop_sequence: Some(stop_sequence),
            arrival: Some(StopTimeEvent {
                time: Some(arrival_time),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn selected_sequence(stus: &[StopTimeUpdate], now_secs: i64) -> Option<u32> {
        select_next_stop(stus, None, now_secs).and_then(|stu| stu.stop_sequence)
    }

    // Trip 0_11_3302_33_10017, vehicle 383, feed processed at about 00:55:00.
    // Seq 7 is stale: its arrival is after the reached seq 9.
    #[test]
    fn skips_stale_update_before_reached_stop() {
        let now = 1_795_650_900;
        let stus = [
            stu(7, 1_795_650_871),
            stu(9, 1_795_650_805),
            stu(10, 1_795_650_893),
        ];

        assert_eq!(selected_sequence(&stus, now), Some(10));
    }

    #[test]
    fn selects_first_update_when_none_are_reached() {
        let stus = [stu(1, 1_000), stu(2, 1_100)];

        assert_eq!(selected_sequence(&stus, 990), Some(1));
    }

    #[test]
    fn keeps_last_update_when_all_are_reached() {
        let stus = [stu(1, 100), stu(2, 200)];

        assert_eq!(selected_sequence(&stus, 1_000), Some(2));
    }
}
