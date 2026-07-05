import json
import os

hub_path = r"C:\Users\Aryan\.cluaiz"
perm_path = os.path.join(hub_path, "engine", "config", "Permission.json")

try:
    with open(perm_path, 'r') as f:
        perm_json = json.load(f)
        
    active_id = perm_json.get("chat_models", {}).get("text", "").replace(':', '-')
    print(f"active_id: {active_id}")
    
    models_root = os.path.join(hub_path, "models")
    categories = ["chat", "embedding", "vision", "audio", "code"]
    
    found = False
    for category in categories:
        model_dir = os.path.join(models_root, category, active_id)
        print(f"Checking dir: {model_dir}")
        if os.path.isdir(model_dir):
            for entry in os.listdir(model_dir):
                print(f"Found file: {entry}")
                if entry.endswith(".gguf"):
                    print(f"Match: {os.path.join(model_dir, entry)}")
                    found = True
                    break
        if found:
            break
            
    if not found:
        print("Returned None")
except Exception as e:
    print(f"Error: {e}")
