#pragma once
// Sort only the displayed torrent-file indices. Actual torrent priorities stay
// keyed to libtorrent's original file indices, regardless of display order.
#include <windows.h>
#include <algorithm>
#include <cstdint>
#include <numeric>
#include <string>
#include <vector>

namespace filelist {
inline int compareNames(std::wstring const& a,std::wstring const& b) {
 int result=CompareStringEx(LOCALE_NAME_USER_DEFAULT,NORM_IGNORECASE|SORT_DIGITSASNUMBERS,
  a.c_str(),-1,b.c_str(),-1,nullptr,nullptr,0);
 if(result==0)result=CompareStringOrdinal(a.c_str(),-1,b.c_str(),-1,TRUE);
 return result==CSTR_LESS_THAN?-1:result==CSTR_GREATER_THAN?1:0;
}
inline std::vector<int> sortedIndices(std::vector<std::wstring> const& names,
 std::vector<std::int64_t> const& sizes,int sortBy,bool ascending) {
 std::vector<int> indices(names.size());
 std::iota(indices.begin(),indices.end(),0);
 if(sortBy!=1&&sortBy!=2)return indices;
 std::stable_sort(indices.begin(),indices.end(),[&](int a,int b){
  int result=0;
  if(sortBy==1)result=compareNames(names[a],names[b]);
  else if(a<int(sizes.size())&&b<int(sizes.size()))
   result=sizes[a]<sizes[b]?-1:sizes[a]>sizes[b]?1:0;
  return ascending?result<0:result>0;
 });
 return indices;
}
} // namespace filelist
