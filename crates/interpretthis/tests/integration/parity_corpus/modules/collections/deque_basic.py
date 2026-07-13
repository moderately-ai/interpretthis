# deque basic ops: append, appendleft, pop, popleft, extend, rotate, clear.
import collections
d = collections.deque([1, 2, 3])
print(d)
d.append(4)
print(d)
d.appendleft(0)
print(d)
print(d.pop())          # 4
print(d.popleft())      # 0
print(d)
d.extend([4, 5])
print(d)
d.extendleft([0, -1])   # reverses: -1 first into front, then 0 into front
print(d)
d.rotate(2)
print(d)
d.rotate(-2)
print(d)
print(len(d))
print(3 in d)
d.clear()
print(d)
print(len(d))
