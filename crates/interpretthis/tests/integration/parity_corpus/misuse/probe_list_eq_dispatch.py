class Item:
    def __init__(self, id):
        self.id = id
    def __eq__(self, o):
        return isinstance(o, Item) and self.id == o.id
    def __repr__(self):
        return f"Item({self.id})"
items = [Item(1), Item(2), Item(3)]
print(Item(2) in items)
print(items.index(Item(2)))
print(items.count(Item(2)))
items.remove(Item(2))
print(items)
print(Item(5) in items)
lst = [Item(1), Item(1), Item(2)]
print(lst.count(Item(1)))
tup = (Item(1), Item(2))
print(Item(1) in tup)
print(tup.index(Item(2)))
