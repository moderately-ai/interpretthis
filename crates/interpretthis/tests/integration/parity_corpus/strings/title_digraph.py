# str.title()/str.capitalize() use the Unicode titlecase mapping for word-
# initial chars, so the Latin digraph letters map to their mixed-case titlecase
# form (Dž = U+01C5), not the all-caps uppercase (DŽ = U+01C4). Georgian
# Mkhedruli has no titlecase and stays lowercase.
print("ǅ".upper(), "ǅ".lower(), "ǅ".title())
print("ǆ".title(), "ǉ".title(), "ǌ".title(), "ǳ".title())
print("ǆ".capitalize())
print("džul lju njam".title())
print("ǆ dž".title())
print("ა".title(), "ბ".title())
print("hello world".title(), "2nd place".title())
