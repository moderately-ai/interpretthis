# Pins: list.sort(key=fn) — method form of sorted, mutates in place.
# Same gap as `sorted` had before A3 — the kwarg is silently ignored
# by the method-dispatch path because dispatch_method is positional-only.
xs = [1, 10, 2, 20, 3]
xs.sort(key=str)
print(xs)
