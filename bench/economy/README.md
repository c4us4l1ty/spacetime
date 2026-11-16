# Mock economy

`bounties.json` is a local bounty board over the 20 offline EconBench seed fixtures.
`mock_wallet.jsonl` and `mock_wallet.jsonl.head` form a real append-only hash-chained ledger with
mock credit and payment-confirmation events. Credits are fake, no wallet or account is real, no
payment leaves the process, and no public internet is used. These two checked-in offline fixtures
have narrow `.gitignore` exceptions; generated ledgers remain ignored.

Verify locally with:

```sh
spacetime-ctl ledger verify bench/economy/mock_wallet.jsonl
```

This is evidence-preparation plumbing only; it is not a measured resource gate and not a
performance claim.
