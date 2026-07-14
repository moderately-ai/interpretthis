import heapq
h = [3, 1, 4, 1, 5]
heapq.heapify(h)
print(heapq.heappop(h))
heapq.heappush(h, 0)
print(heapq.heappop(h))
print(heapq.nsmallest(2, [5, 3, 8, 1]))
print(heapq.nlargest(2, [5, 3, 8, 1]))
import bisect
a = [1, 3, 5, 7]
print(bisect.bisect_left(a, 4))
bisect.insort(a, 4)
print(a)
