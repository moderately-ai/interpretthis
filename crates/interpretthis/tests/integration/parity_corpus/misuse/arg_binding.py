def f(a, b):
    return a + b
for call in ["extra", "dup", "unknown", "missing"]:
    try:
        if call == "extra": f(1, 2, 3)
        elif call == "dup": f(1, a=2)
        elif call == "unknown": f(1, 2, c=3)
        elif call == "missing": f(1)
        print(call, "no-error")
    except TypeError:
        print(call, "TypeError")
