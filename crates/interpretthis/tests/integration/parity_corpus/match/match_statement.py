# Pin: basic `match`/`case` literal-pattern dispatch with wildcard fallback.
# Expected stdout: `two`.
x = 2
match x:
    case 1:
        print("one")
    case 2:
        print("two")
    case _:
        print("other")
