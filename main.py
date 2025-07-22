import vroomrs

# with open("/Users/vigliasentry/Desktop/tr_prof_950a1ebb1e664a17b4cd88e0728a4173", "rb") as f:
#     p = vroomrs.decompress_profile(f.read())

with open("/Users/vigliasentry/Downloads/sentry_sentry_1aabc37c83084af8a89efba76b513b2d.profile.json", "r") as f:
    p = vroomrs.profile_from_json_str(f.read(), "python")

print(p.get_platform())
print(p.get_project_id())
print(p.get_organization_id())

occ = p.find_occurrences()
occ.filter_none_type_issues()
print(len(occ.occurrences))
for occurrence in occ.occurrences:
    print(occurrence.to_json_str())
