#include "file_list_sort.h"
#include <cstdint>
#include <string>
#include <vector>

int main() {
 // Natural, case-insensitive ordering: episode 2 precedes episode 10.
 std::vector<std::wstring> names={L"Episode 10.mkv",L"episode 2.mkv",L"Episode 1.mkv"};
 std::vector<std::int64_t> sizes={500,100,300};
 if(filelist::sortedIndices(names,sizes,1,true)!=std::vector<int>{2,1,0})return 1;
 if(filelist::sortedIndices(names,sizes,1,false)!=std::vector<int>{0,1,2})return 2;
 if(filelist::sortedIndices(names,sizes,2,true)!=std::vector<int>{1,2,0})return 3;
 if(filelist::sortedIndices(names,sizes,2,false)!=std::vector<int>{0,2,1})return 4;
 if(filelist::sortedIndices(names,sizes,0,true)!=std::vector<int>{0,1,2})return 5;
 // Keep original file indexes when sorting: selection and priorities must not drift.
 std::vector<int> originalPriorities={0,7,1};
 auto order=filelist::sortedIndices(names,sizes,1,true);
 if(originalPriorities[order[0]]!=1||originalPriorities[order[1]]!=7||originalPriorities[order[2]]!=0)return 6;
 std::vector<std::wstring> russian={L"Серия 12.mkv",L"Серия 3.mkv"};
 if(filelist::sortedIndices(russian,{1,1},1,true)!=std::vector<int>{1,0})return 7;
 return 0;
}
