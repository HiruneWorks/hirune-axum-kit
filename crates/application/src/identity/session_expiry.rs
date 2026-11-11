use crate::{ApplicationError, ApplicationResult};
use chrono::{DateTime, Duration, Utc};

pub(crate) fn session_expires_at(
    now: DateTime<Utc>,
    lifetime_seconds: i64,
) -> ApplicationResult<DateTime<Utc>> {
    let lifetime = Duration::try_seconds(lifetime_seconds)
        .filter(|duration| *duration > Duration::zero())
        .ok_or(ApplicationError::InvalidInput("invalid session lifetime"))?;
    now.checked_add_signed(lifetime)
        .ok_or(ApplicationError::InvalidInput(
            "session lifetime exceeds the supported date range",
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_rejects_nonpositive_or_unrepresentable_durations() {
        let now = DateTime::from_timestamp(0, 0).unwrap();
        for lifetime_seconds in [i64::MIN, -1, 0, i64::MAX, Duration::MAX.num_seconds()] {
            assert!(matches!(
                session_expires_at(now, lifetime_seconds),
                Err(ApplicationError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn expiry_accepts_the_last_supported_timestamp_and_rejects_the_next_second() {
        let now = DateTime::<Utc>::MAX_UTC - Duration::seconds(60);
        assert_eq!(
            session_expires_at(now, 60).unwrap(),
            DateTime::<Utc>::MAX_UTC
        );
        assert!(matches!(
            session_expires_at(now, 61),
            Err(ApplicationError::InvalidInput(_))
        ));
    }
}
