import sys
import requests
import os
import time

# --- Configuration & Constants ---
GITHUB_API_URL = "https://api.github.com"
OPENAI_THREAD_URL = "https://api.openai.com/v1/threads"
OPENAI_ASSISTANT_ID = "asst_JUTY7WIQ6hsKWEKWcVhQL78u"
MODEL_NAME = "gpt4o-mini"
MIN_DIFF_SIZE = 75

# Prompt: Desktop review version for TauriV2 + React19
BASE_PROMPT = (
    "You are a seasoned code reviewer. Please analyze the following cumulative code diff - for a TauriV2/React19 time tracking application - and provide a strong but to the point review for the PR. Only comment on changes directly introduced in the diff — ignore unrelated assumptions or suggestions or hallucinations. Follow these instructions regarding content perfectly, do not hallucinate and ensure that you are following the directions as a whole since they apply to each section. Format your response in Markdown with the following structure:\n\n"
    "# PR Code Review Analysis\n\n"
    "## Summary:\nConcise summary of the changes introduced in the diff. No additional comments. No points about adding todos. No points about changing ENVs. No points about double checking.\n\n"
    "## Changes:\nTo the point bullet points listing only functional code changes. Ignore formatting, styling, test updates, or unrelated improvements. Write the file name after the period of each bullet point.\n\n"
    "## Detailed Observations:\nBullet points listing only functional issues or potential bugs directly introduced in the diff. No generic suggestions (like check accessibility or verify behavior). No points about changing styles or adding todos.\n\n"
    "## Fixes and Improvements:\nBullet points listing actionable recommendations for fixes. We are using React19, TailwindV4, and TauriV2/Rust. Only include specific, value-adding improvements or corrections related to core functionalities that appear in the diff. Write the file name for each bullet point at the end in parentheses.\n"
)

# Retrieve configuration from environment variables
OWNER = os.getenv("OWNER")
REPO = os.getenv("REPO")
PR_NUMBER = os.getenv("PR_NUMBER")
GITHUB_TOKEN = os.getenv("GITHUB_TOKEN")
OPENAI_API_KEY = os.getenv("OPENAI_API_KEY")

# --- Helper Functions ---
def get_changed_files():
    url = f"{GITHUB_API_URL}/repos/{OWNER}/{REPO}/pulls/{PR_NUMBER}/files"
    headers = {
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    response = requests.get(url, headers=headers)
    response.raise_for_status()
    return response.json()

def calculate_diff_size(diff_text: str):
    return diff_text.count("\n+") + diff_text.count("\n-")

def openai_headers():
    return {
        "Authorization": f"Bearer {OPENAI_API_KEY}",
        "Content-Type": "application/json",
        "OpenAI-Beta": "assistants=v2"
    }

def create_thread():
    res = requests.post(OPENAI_THREAD_URL, headers=openai_headers(), json={})
    res.raise_for_status()
    return res.json()["id"]

def add_message(thread_id, content):
    url = f"{OPENAI_THREAD_URL}/{thread_id}/messages"
    requests.post(url, headers=openai_headers(), json={"role": "user", "content": content}).raise_for_status()

def run_assistant(thread_id):
    url = f"{OPENAI_THREAD_URL}/{thread_id}/runs"
    res = requests.post(url, headers=openai_headers(), json={"assistant_id": OPENAI_ASSISTANT_ID})
    res.raise_for_status()
    return res.json()["id"]

def wait_for_completion(thread_id, run_id):
    url = f"{OPENAI_THREAD_URL}/{thread_id}/runs/{run_id}"
    while True:
        res = requests.get(url, headers=openai_headers())
        status = res.json()["status"]
        if status in ["completed", "failed"]:
            return status
        time.sleep(2)

def fetch_response(thread_id):
    url = f"{OPENAI_THREAD_URL}/{thread_id}/messages"
    return requests.get(url, headers=openai_headers()).json()["data"][0]["content"][0]["text"]["value"]

def post_comment(review):
    url = f"{GITHUB_API_URL}/repos/{OWNER}/{REPO}/issues/{PR_NUMBER}/comments"
    requests.post(url, headers={
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }, json={"body": review}).raise_for_status()

def main():
    files = get_changed_files()
    ignore_ext = ('.yml', '.css', '.json', '.lock', '.env', '.txt', '.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico',
                  '.ttf', '.woff', '.woff2', '.eot', '.otf', '.webp', '.md', '.htm', '.xml', '.jsonld', '.csv', '.yaml', '.toml')

    aggregated_diff, total_diff_size = '', 0
    for f in files:
        if not f.get("patch") or f["filename"].lower().endswith(ignore_ext):
            continue
        aggregated_diff += f"\n### File: {f['filename']}\n{f['patch']}\n"
        total_diff_size += calculate_diff_size(f['patch'])

    if total_diff_size < MIN_DIFF_SIZE:
        post_comment(f"Skipping AI review: Diff size ({total_diff_size}) is below threshold.")
        return

    thread_id = create_thread()
    add_message(thread_id, BASE_PROMPT + "\n\n" + aggregated_diff)
    run_id = run_assistant(thread_id)
    if wait_for_completion(thread_id, run_id) == "completed":
        review = fetch_response(thread_id)
        post_comment(review)
        print("Review posted successfully.")
    else:
        print("Assistant run failed.")

if __name__ == "__main__":
    main()