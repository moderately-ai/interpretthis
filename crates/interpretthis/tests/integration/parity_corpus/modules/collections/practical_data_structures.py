import heapq
class PriorityQueue:
    def __init__(self):
        self._heap = []
        self._count = 0
    def push(self, item, priority):
        heapq.heappush(self._heap, (priority, self._count, item))
        self._count += 1
    def pop(self):
        return heapq.heappop(self._heap)[2]
pq = PriorityQueue()
pq.push("low", 3)
pq.push("high", 1)
pq.push("mid", 2)
print(pq.pop(), pq.pop(), pq.pop())
tasks = [(3, "c"), (1, "a"), (2, "b")]
heapq.heapify(tasks)
print([heapq.heappop(tasks) for _ in range(3)])
print(heapq.nlargest(2, [1, 5, 3, 8, 2]))
print(heapq.nsmallest(2, [1, 5, 3, 8, 2]))
h = [1, 2, 3]
print(heapq.heappushpop(h, 0))
print(heapq.heapreplace([1, 2, 3], 5))
from collections import deque
dq = deque(maxlen=3)
for i in range(5):
    dq.append(i)
print(list(dq))
q = deque()
q.append(1)
q.append(2)
q.appendleft(0)
print(q.popleft(), q.pop())
stack = deque()
stack.append("a")
stack.append("b")
print(stack.pop())
sliding = deque(maxlen=3)
results = []
for x in [1, 2, 3, 4, 5]:
    sliding.append(x)
    if len(sliding) == 3:
        results.append(sum(sliding))
print(results)
from collections import Counter
words = "the quick brown fox the lazy dog the".split()
wc = Counter(words)
print(wc.most_common(2))
print(wc["the"])
c1 = Counter("aabbcc")
c2 = Counter("abccdd")
print(dict(c1 + c2))
print(dict(c1 - c2))
print(dict(c1 & c2))
votes = Counter()
for v in ["a", "b", "a", "c", "a", "b"]:
    votes[v] += 1
print(votes.most_common())
from collections import OrderedDict
od = OrderedDict()
od["z"] = 1
od["a"] = 2
od["m"] = 3
print(list(od.items()))
od.move_to_end("z")
print(list(od.keys()))
od.move_to_end("a", last=False)
print(list(od.keys()))
lru = OrderedDict()
def access(key, value=None):
    if key in lru:
        lru.move_to_end(key)
    if value is not None:
        lru[key] = value
    if len(lru) > 3:
        lru.popitem(last=False)
access("a", 1)
access("b", 2)
access("c", 3)
access("a")
access("d", 4)
print(list(lru.keys()))
from collections import defaultdict
graph = defaultdict(list)
edges = [(1, 2), (1, 3), (2, 3), (3, 1)]
for u, v in edges:
    graph[u].append(v)
print(dict(graph))
groups = defaultdict(list)
for word in ["apple", "banana", "avocado", "cherry", "blueberry"]:
    groups[word[0]].append(word)
print(dict(groups))
counts = defaultdict(int)
for c in "mississippi":
    counts[c] += 1
print(sorted(counts.items()))
nested = defaultdict(lambda: defaultdict(int))
nested["a"]["x"] += 1
nested["a"]["y"] += 2
nested["b"]["x"] += 3
print({k: dict(v) for k, v in nested.items()})
import bisect
sorted_list = [1, 3, 5, 7, 9]
bisect.insort(sorted_list, 4)
print(sorted_list)
print(bisect.bisect_left(sorted_list, 5), bisect.bisect_right(sorted_list, 5))
scores = [60, 70, 80, 90]
def grade(score):
    return ["F", "D", "C", "B", "A"][bisect.bisect(scores, score)]
print([grade(s) for s in [55, 65, 75, 85, 95]])
