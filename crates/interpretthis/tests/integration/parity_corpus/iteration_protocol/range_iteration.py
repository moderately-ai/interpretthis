# Range iteration routes through types::range_iter (was the legacy
# direct-match arm). Pins the step-aware walk + the empty-range cases.
print(list(range(5)))
print(list(range(0, 10, 2)))
print(list(range(10, 0, -1)))
print(list(range(0)))           # empty
print(list(range(5, 5)))        # empty (start == stop)
print(list(range(0, -5, -1)))
print(sum(range(100)))           # sum() consumes via dispatch_iter
