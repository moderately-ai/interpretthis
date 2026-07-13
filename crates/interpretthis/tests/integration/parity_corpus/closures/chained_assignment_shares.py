# Pins: chained assignment to a list literal shares the same list
# identity across both targets. Mutating through one alias is observable
# through the other — D2 (SharedList = Arc<Mutex<Vec<Value>>>) closes
# the gap that previously made `b` an independent clone.
a = b = []
a.append(1)
print(b)
