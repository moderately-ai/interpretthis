def describe(x):
    match x:
        case 0:
            return "zero"
        case [a, b]:
            return f"pair {a},{b}"
        case [a, *rest]:
            return f"list starting {a}, rest {rest}"
        case {"key": v}:
            return f"dict with key={v}"
        case str() as s:
            return f"string {s}"
        case int() if x > 100:
            return "big int"
        case _:
            return "other"
print(describe(0))
print(describe([1, 2]))
print(describe([1, 2, 3, 4]))
print(describe({"key": "val"}))
print(describe("hi"))
print(describe(500))
print(describe(3.14))
