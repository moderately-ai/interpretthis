Truthiness rules: `bool` dispatch through `__bool__`, fall-back to `__len__() != 0`, fall-back to `True`. Falsy values: `0`, `0.0`, `""`, `[]`, `{}`, `set()`, `None`.
