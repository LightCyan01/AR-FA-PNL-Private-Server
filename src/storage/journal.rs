// Retention and replay-journal maintenance.

use rusqlite::params;

use super::{StorageError, Store, JOURNAL_RETENTION_COUNT, JOURNAL_RETENTION_SECONDS};

impl Store {
    pub fn prune_expired(&self, now: i64) -> Result<(), StorageError> {
        let connection = self.lock_connection();
        connection.execute(
            "DELETE FROM auth_grants WHERE grant_id IN (
                 SELECT grant_id FROM auth_grants WHERE expires_at <= ?1 LIMIT 128
             )",
            [now],
        )?;
        connection.execute(
            "DELETE FROM sessions WHERE session_id IN (
                 SELECT session_id FROM sessions WHERE expires_at <= ?1 LIMIT 128
             )",
            [now],
        )?;
        connection.execute(
            "DELETE FROM gameplay_mutations WHERE mutation_id IN (
                 SELECT stale.mutation_id FROM gameplay_mutations AS stale
                 WHERE stale.mutation_id != COALESCE((
                 SELECT recovery.mutation_id
                 FROM active_battles AS active
                 JOIN gameplay_mutations AS recovery ON recovery.account_id=active.account_id
                 WHERE active.account_id=stale.account_id
                 AND recovery.route IN ('/quest/battle/start','/quest/battle/rental_party_start','/exploration/battle_start','/quest/battle/solo_raid_battle_start','/quest/battle/total_battle_start','/gacha/battle_start')
                 ORDER BY recovery.mutation_id DESC LIMIT 1
             ), -1)
                 AND (stale.applied_at < ?1 OR (
                 SELECT COUNT(*) FROM gameplay_mutations AS newer
                 WHERE newer.account_id=stale.account_id AND newer.mutation_id > stale.mutation_id
                 ) >= ?2)
                 LIMIT 128
             )",
            params![now - JOURNAL_RETENTION_SECONDS, JOURNAL_RETENTION_COUNT],
        )?;
        connection.execute(
            "DELETE FROM memoria_deltas WHERE delta_id IN (
                 SELECT stale.delta_id FROM memoria_deltas AS stale
                 WHERE stale.applied_at < ?1 OR (
                 SELECT COUNT(*) FROM memoria_deltas AS newer
                 WHERE newer.account_id=stale.account_id AND newer.delta_id > stale.delta_id
                 ) >= ?2
                 LIMIT 128
             )",
            params![now - JOURNAL_RETENTION_SECONDS, JOURNAL_RETENTION_COUNT],
        )?;
        Ok(())
    }
}
