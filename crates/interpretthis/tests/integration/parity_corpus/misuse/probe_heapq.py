import heapq
h = [3, 1, 4, 1, 5, 9, 2, 6]
heapq.heapify(h)
print(h[0])
print(heapq.heappop(h))
heapq.heappush(h, 0)
print(h[0])
print(heapq.nlargest(3, [1,5,3,8,2,9,4]))
print(heapq.nsmallest(3, [1,5,3,8,2,9,4]))
print(heapq.heappushpop(h, 7))
print(sorted([heapq.heappop(h) for _ in range(len(h))]))
data = [5, 7, 9, 1, 3]
heapq.heapify(data)
result = []
while data:
    result.append(heapq.heappop(data))
print(result)
print(heapq.nlargest(2, [{"v": 3}, {"v": 1}, {"v": 2}], key=lambda x: x["v"]))
print(list(heapq.merge([1,3,5], [2,4,6])))
