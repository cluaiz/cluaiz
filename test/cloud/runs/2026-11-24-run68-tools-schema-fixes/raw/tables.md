
out-before
| Case | Valid | Constrained (log) | HTTP | prompt_n |
|---|---:|---:|---|---|
| v_img_tool | 0 / 10 | 0 / 10 | [200] | [225] |
| v_txt_tool | 10 / 10 | 0 / 10 | [200] | [284] |
| v_img_red | 10 / 10 | 0 / 10 | [200] | [219] |
| b_json_object | 10 / 10 | 10 / 10 | [200] | [22] |
| c_json_schema | 10 / 10 | 10 / 10 | [200] | [22] |
| d_grammar | 10 / 10 | 10 / 10 | [200] | [23] |
| e_tool_required | 10 / 10 | 0 / 10 | [200] | [157] |
| f_json_schema_with_tool | 6 / 10 | 0 / 10 | [200] | [161] |
| h_required_unrelated | 0 / 10 | 0 / 10 | [200] | [157] |
| g_grammar_with_tool | 0 / 10 | 0 / 10 | [200] | [162] |
| g2_grammar_engine_functions | 0 / 10 | 0 / 10 | [200] | [484] |
| j_schema_engine_functions | 0 / 10 | 0 / 10 | [200] | [61, 62, 66] |
| f2_schema_tool_weather_question | 10 / 10 | 0 / 10 | [200] | [12, 157] |
| i_named_get_time | 0 / 10 | 0 / 10 | [200] | [212] |
| i2_named_unknown | 0 / 10 | 0 / 10 | [200] | [149] |

out-after1
| Case | Valid | Constrained (log) | HTTP | prompt_n |
|---|---:|---:|---|---|
| v_img_tool | 10 / 10 | 0 / 10 | [200] | [482] |
| v_txt_tool | 10 / 10 | 0 / 10 | [200] | [13, 284] |
| v_img_red | 10 / 10 | 0 / 10 | [200] | [219] |
| b_json_object | 10 / 10 | 10 / 10 | [200] | [22] |
| c_json_schema | 10 / 10 | 10 / 10 | [200] | [22] |
| d_grammar | 10 / 10 | 10 / 10 | [200] | [23] |
| e_tool_required | 10 / 10 | 0 / 10 | [200] | [10, 157] |
| f2_schema_tool_weather_question | 10 / 10 | 0 / 10 | [200] | [12, 157] |

out-after2
| Case | Valid | Constrained (log) | HTTP | prompt_n |
|---|---:|---:|---|---|
| v_img_tool | 10 / 10 | 0 / 10 | [200] | [482] |
| v_txt_tool | 10 / 10 | 0 / 10 | [200] | [13] |
| v_img_red | 10 / 10 | 0 / 10 | [200] | [219] |
| b_json_object | 10 / 10 | 10 / 10 | [200] | [22] |
| c_json_schema | 10 / 10 | 10 / 10 | [200] | [22] |
| d_grammar | 10 / 10 | 10 / 10 | [200] | [23] |
| e_tool_required | 10 / 10 | 10 / 10 | [200] | [157] |
| f_json_schema_with_tool | 10 / 10 | 10 / 10 | [200] | [161] |
| h_required_unrelated | 2 / 10 | 10 / 10 | [200] | [157] |
| g_grammar_with_tool | 10 / 10 | 0 / 10 | [400] | [] |
| g2_grammar_engine_functions | 10 / 10 | 10 / 10 | [200] | [25] |
| j_schema_engine_functions | 10 / 10 | 10 / 10 | [200] | [] |
| f2_schema_tool_weather_question | 10 / 10 | 10 / 10 | [200] | [157] |
| i_named_get_time | 10 / 10 | 10 / 10 | [200] | [157] |
| i2_named_unknown | 10 / 10 | 0 / 10 | [400] | [] |

out-final
| Case | Valid | Constrained (log) | HTTP | prompt_n |
|---|---:|---:|---|---|
| v_img_tool | 3 / 3 | 0 / 3 | [200] | [13, 482] |
| c_json_schema | 3 / 3 | 3 / 3 | [200] | [22] |
| e_tool_required | 3 / 3 | 3 / 3 | [200] | [157] |
| f_json_schema_with_tool | 3 / 3 | 3 / 3 | [200] | [161] |
| g_grammar_with_tool | 3 / 3 | 0 / 3 | [400] | [] |
| g2_grammar_engine_functions | 3 / 3 | 3 / 3 | [200] | [] |
| j_schema_engine_functions | 3 / 3 | 3 / 3 | [200] | [] |
| i_named_get_time | 3 / 3 | 3 / 3 | [200] | [157] |
