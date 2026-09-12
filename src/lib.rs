#![forbid(unsafe_code)]

pub mod api;
pub mod assets;
pub mod config;
pub mod state;
pub mod storage;
pub mod transport;

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use crate::{
        storage::{GameplayMutationResult, MutationResult, StorageError, Store},
        transport::{decrypt_frame, encrypt_frame},
    };

    #[test]
    fn transport_vectors_and_persistent_idempotency() {
        let vectors = [
            (
                0u8,
                b"\x08\x01".as_slice(),
                "0032ece49c071767c6347cf89a7e74023f",
            ),
            (
                46u8,
                b"\x08\x01".as_slice(),
                "2ed26237c0fd0c7f53ff615fb302978f8c",
            ),
            (
                148u8,
                b"\x08\xb7\xce\x94\x30\x10\x01".as_slice(),
                "940e935864c9b819138ca91fb26ac1e4ab",
            ),
        ];
        for (prefix, plaintext, expected) in vectors {
            let expected = hex(expected);
            assert_eq!(encrypt_frame(prefix, plaintext).unwrap(), expected);
            assert_eq!(
                decrypt_frame(&expected).unwrap(),
                (prefix, plaintext.to_vec())
            );
        }

        let path = env::temp_dir().join(format!("atelier-stage6-{}.sqlite3", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        store
            .create_account("test_user", "$argon2id$v=19$test")
            .unwrap();
        store.ensure_character(1, 43101).unwrap();
        let fingerprint = [7u8; 32];
        let response = b"canonical";
        let first = store
            .apply_memoria(1, "request-1", &fingerprint, 43101, Some(9), response, 1)
            .unwrap();
        assert!(matches!(first, MutationResult::Applied { .. }));
        let replay = store
            .apply_memoria(
                1,
                "request-1",
                &fingerprint,
                43101,
                Some(9),
                b"different",
                2,
            )
            .unwrap();
        assert!(matches!(replay, MutationResult::Replay(bytes) if bytes == response));
        assert!(store
            .apply_memoria(1, "request-1", &[8u8; 32], 43101, Some(10), response, 3)
            .is_err());
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.characters(1).unwrap(), vec![(43101, Some(9))]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn gameplay_mutation_commits_and_replays_after_reopen() {
        let path =
            env::temp_dir().join(format!("atelier-stage7b-{}.sqlite3", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        store
            .create_account("quest_test", "$argon2id$v=19$test")
            .unwrap();
        store.ensure_player_state(1, b"old", "jp").unwrap();

        let fingerprint = [7u8; 32];
        let result = store
            .apply_gameplay(
                1,
                "quest-1",
                "/quest/talk_event/finish",
                &fingerprint,
                b"new",
                Some(32201),
                b"response",
                1,
            )
            .unwrap();
        assert_eq!(result, GameplayMutationResult::Applied);
        drop(store);

        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.player_resources(1).unwrap(), b"new");
        assert_eq!(reopened.characters(1).unwrap(), vec![(32201, None)]);
        assert_eq!(
            reopened
                .gameplay_replay(1, "quest-1", "/quest/talk_event/finish", &fingerprint,)
                .unwrap(),
            Some(b"response".to_vec())
        );
        assert!(matches!(
            reopened.gameplay_replay(1, "quest-1", "/quest/talk_event/finish", &[8u8; 32]),
            Err(StorageError::RequestConflict)
        ));
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    fn hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
