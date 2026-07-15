import heapq
h = [3, 1, 4, 1, 5, 9, 2, 6]
heapq.heapify(h)
print(heapq.heappop(h))
heapq.heappush(h, 0)
print(heapq.heappop(h))
print(heapq.nlargest(3, [1, 5, 2, 8, 3]))
print(heapq.nsmallest(2, [1, 5, 2, 8, 3]))
print(heapq.heappushpop([1, 2, 3], 0))
data = [5, 3, 8, 1]
heapq.heapify(data)
print(heapq.heapreplace(data, 4))
result = []
h2 = []
for x in [3, 1, 2]:
    heapq.heappush(h2, x)
while h2:
    result.append(heapq.heappop(h2))
print(result)
print(list(heapq.merge([1, 3, 5], [2, 4, 6])))
