from collections import deque
d = deque()
try:
    d.pop()
except IndexError as e:
    print("pop:", type(e).__name__)
try:
    d.popleft()
except IndexError as e:
    print("popleft:", type(e).__name__)
