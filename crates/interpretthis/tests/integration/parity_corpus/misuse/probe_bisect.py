import bisect
a = [1, 3, 5, 7, 9]
print(bisect.bisect_left(a, 5))
print(bisect.bisect_right(a, 5))
print(bisect.bisect(a, 6))
bisect.insort(a, 4)
print(a)
bisect.insort_left(a, 4)
print(a)
print(bisect.bisect_left([1,2,2,2,3], 2))
print(bisect.bisect_right([1,2,2,2,3], 2))
b = []
for x in [5, 1, 3, 2, 4]:
    bisect.insort(b, x)
print(b)
print(bisect.bisect_left([], 5))
print(bisect.bisect(a, 100))
grades = "FEDCBA"
breakpoints = [30, 44, 66, 75, 85]
print(grades[bisect.bisect(breakpoints, 70)])
