try:
    raise ExceptionGroup("group", [ValueError("v"), TypeError("t")])
except* ValueError as eg:
    print("caught value", len(eg.exceptions))
except* TypeError as eg:
    print("caught type", len(eg.exceptions))
try:
    raise ExceptionGroup("nested", [ValueError("a"), ValueError("b"), KeyError("k")])
except* ValueError as eg:
    print("values:", len(eg.exceptions))
except* KeyError as eg:
    print("keys:", len(eg.exceptions))
