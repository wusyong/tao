---
"tao": "minor"
---

Remove `instant` dependency, changed `StartCause::ResumeTimeReached`, `StartCause::WaitCancelled` and `ControlFlow::WaitUntil` to use `std::time::Instant` instead.
