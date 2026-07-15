from collections import deque
print(deque([1,2,3]) == deque([1,2,3]))
print(deque([1,2,3]) == deque([1,2,4]))
print(deque([1,2]) == deque([1,2,3]))
print(deque() == deque())
d1 = deque([1,2,3], maxlen=5)
d2 = deque([1,2,3])
print(d1 == d2)
print(deque([1,2,3]) != deque([3,2,1]))
