# math.hypot is the n-dimensional Euclidean norm. Regression: it read only the
# first two coordinates, so a third dimension was ignored.
import math

print(math.hypot(3, 4))          # 5.0
print(math.hypot(1, 2, 2))       # 3.0
print(math.hypot(2, 3, 6))       # 7.0
print(math.hypot(5))             # 5.0 (1-D -> abs)
print(math.hypot())              # 0.0
print(round(math.hypot(1, 1, 1, 1), 4))
