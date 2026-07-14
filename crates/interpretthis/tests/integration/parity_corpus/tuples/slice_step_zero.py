# A slice step of 0 raises ValueError, including via `False` (bool is an int
# subclass). Regression: `a[::False]` was a TypeError on read and a silent
# stride-1 on assignment.
a = [1, 2, 3, 4]
print(a[::True])            # step 1
print(a[::2])

try:
    a[::0]
except ValueError:
    print("read0 ValueError")
try:
    a[::False]
except ValueError:
    print("readF ValueError")
try:
    "abc"[::0]
except ValueError:
    print("str ValueError")
try:
    (1, 2, 3)[::False]
except ValueError:
    print("tuple ValueError")

# Slice assignment with a zero/False step also raises.
b = [1, 2, 3, 4]
try:
    b[::0] = [9]
except ValueError:
    print("assign0 ValueError")
c = [1, 2, 3, 4]
try:
    c[::False] = [9]
except ValueError:
    print("assignF ValueError")
