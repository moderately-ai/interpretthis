# deque.pop / popleft on an empty deque raise IndexError (not RuntimeError), so
# `except IndexError` catches them as CPython code expects.
from collections import deque

dq = deque()
try:
    dq.pop()
except IndexError:
    print("pop IndexError")

try:
    dq.popleft()
except IndexError:
    print("popleft IndexError")

# A non-empty deque still pops normally.
dq2 = deque([1, 2, 3])
print(dq2.pop(), dq2.popleft(), list(dq2))
