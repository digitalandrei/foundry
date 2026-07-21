export type SearchablePickerMatchOption = {
  label: string
  searchText?: string
  group?: string
  subgroup?: string
}

export function searchablePickerMatches(option: SearchablePickerMatchOption, query: string) {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean)
  if (terms.length === 0) return true
  const haystack = [option.label, option.searchText, option.group, option.subgroup]
    .filter(Boolean)
    .join(" ")
    .toLocaleLowerCase()
  return terms.every((term) => haystack.includes(term))
}
