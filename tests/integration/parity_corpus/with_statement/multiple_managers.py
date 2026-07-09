# Pins: `with A() as a, B() as b:` enters managers left-to-right
# and exits right-to-left. Multi-manager pattern.
class Mock:
    def __init__(self, name): self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self
    def __exit__(self, *args):
        print(f"exit {self.name}")

with Mock("A") as a, Mock("B") as b:
    print(f"body {a.name}/{b.name}")
