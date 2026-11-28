use crate::proto::{
    gtfs_realtime::data::transit_realtime::trip_update::StopTimeUpdate,
    gtfs_schedule::data::trip_path::PathStop,
};

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

/// The next stop of a vehicle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NextStop<'a> {
    /// The feed update for the stop.
    Feed(&'a StopTimeUpdate),
    /// The stop from the vehicle position, when the feed has no usable update
    /// for it.
    Located(&'a PathStop),
}

impl NextStop<'_> {
    pub const fn stop_sequence(self) -> Option<u32> {
        match self {
            Self::Feed(update) => update.stop_sequence,
            Self::Located(stop) => Some(stop.stop_sequence),
        }
    }
}

/// Combines `feed`, the next stop from [`select_next_stop`], with `located`,
/// the next stop from the vehicle position on the trip shape.
///
/// ZET feeds can keep an update for a stop that the vehicle passed long ago.
/// The predicted arrival at that stop then stays about one minute in the
/// future, so the update never counts as reached. When the located stop is
/// after the feed stop, the located stop is used. Its feed update is used if
/// the feed has one.
pub fn combine<'a>(
    feed: Option<&'a StopTimeUpdate>,
    stop_time_updates: &'a [StopTimeUpdate],
    located: Option<&'a PathStop>,
) -> Option<NextStop<'a>> {
    let feed = feed.filter(|update| update.stop_sequence.is_some());
    let Some(located) = located else {
        return feed.map(NextStop::Feed);
    };
    if feed
        .and_then(|update| update.stop_sequence)
        .is_some_and(|sequence| sequence >= located.stop_sequence)
    {
        return feed.map(NextStop::Feed);
    }
    Some(
        stop_time_updates
            .iter()
            .find(|update| update.stop_sequence == Some(located.stop_sequence))
            .map_or(NextStop::Located(located), NextStop::Feed),
    )
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

    fn path_stop(stop_sequence: u32) -> PathStop {
        PathStop {
            stop_sequence,
            stop_id: format!("s{stop_sequence}"),
            along: 0.0,
            arrival_offset: None,
        }
    }

    fn combined_sequence(
        feed: Option<&StopTimeUpdate>,
        stus: &[StopTimeUpdate],
        located: u32,
    ) -> Option<(bool, u32)> {
        let located = path_stop(located);
        combine(feed, stus, Some(&located)).map(|next| {
            (
                matches!(next, NextStop::Feed(_)),
                next.stop_sequence().unwrap_or_default(),
            )
        })
    }

    // Trip 0_11_303_3_11444, vehicle 10349, feed at 23:26:53 CET. The only
    // update is for seq 2, 16 minutes late and one minute in the future. The
    // vehicle is 111 m from seq 17.
    #[test]
    fn located_stop_replaces_stale_feed_stop() {
        let now = 1_795_818_413;
        let stus = [stu(2, 1_795_818_478)];
        let feed = select_next_stop(&stus, None, now);

        assert_eq!(combined_sequence(feed, &stus, 17), Some((false, 17)));
    }

    #[test]
    fn feed_update_for_located_stop_is_used() {
        let stus = [stu(2, 2_000), stu(17, 2_100)];

        assert_eq!(combined_sequence(stus.first(), &stus, 17), Some((true, 17)));
    }

    #[test]
    fn feed_stop_after_located_stop_is_kept() {
        let stus = [stu(5, 2_000)];

        assert_eq!(combined_sequence(stus.first(), &stus, 4), Some((true, 5)));
    }

    #[test]
    fn keeps_last_update_when_all_are_reached() {
        let stus = [stu(1, 100), stu(2, 200)];

        assert_eq!(selected_sequence(&stus, 1_000), Some(2));
    }
}
