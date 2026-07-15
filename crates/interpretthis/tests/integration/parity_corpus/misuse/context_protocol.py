class Resource:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self
    def __exit__(self, exc_type, exc_val, exc_tb):
        print(f"exit {self.name}")
        return False
with Resource("A") as r:
    print(f"using {r.name}")
class Suppress:
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_val, exc_tb):
        return exc_type is ValueError
with Suppress():
    raise ValueError("suppressed")
print("after suppress")
with Resource("X") as x, Resource("Y") as y:
    print("nested")
