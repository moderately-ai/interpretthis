try:
    raise ExceptionGroup("multi", [ValueError("v1"), ValueError("v2"), TypeError("t")])
except* ValueError as eg:
    print("ValueErrors:", len(eg.exceptions))
    print(sorted(str(e) for e in eg.exceptions))
except* TypeError as eg:
    print("TypeErrors:", len(eg.exceptions))
def process():
    errors = []
    for i in range(3):
        try:
            if i == 1:
                raise ValueError(f"error {i}")
        except ValueError as e:
            errors.append(e)
    if errors:
        raise ExceptionGroup("collected", errors)
try:
    process()
except* ValueError as eg:
    print("caught", len(eg.exceptions))
