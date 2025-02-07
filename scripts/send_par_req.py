#!/usr/bin/env python3

import asyncio
from time import perf_counter
import aiohttp
import os
from pathlib import Path
import logging
import json
import glob
import statistics

import argparse
from asyncio.locks import Semaphore

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


def analyze_parsing_results(
    processing_time_s: float,
    results_dir="/tmp/pdf_responses",
):
    stats = {
        "total_documents": 0,
        "total_pages": 0,
        "total_blocks": 0,
        "parsing_durations_ms": [],
        "blocks_per_doc": [],
        "pages_per_doc": [],
        "blocks_per_page": [],
    }

    # Process all JSON files
    for json_file in glob.glob(f"{results_dir}/*.pdf.json"):
        with open(json_file) as f:
            try:
                response = json.load(f)
                if not response.get("success"):
                    continue

                doc = response["data"]

                # Get basic counts
                n_pages = len(doc["pages"])
                n_blocks = len(doc["blocks"])
                parsing_duration_ms = doc["metadata"]["parsing_duration"]

                # Update statistics
                stats["total_documents"] += 1
                stats["total_pages"] += n_pages
                stats["total_blocks"] += n_blocks
                stats["parsing_durations_ms"].append(parsing_duration_ms)
                stats["blocks_per_doc"].append(n_blocks)
                stats["pages_per_doc"].append(n_pages)
                stats["blocks_per_page"].append(
                    n_blocks / n_pages if n_pages > 0 else 0
                )

            except Exception as e:
                print(f"Error processing {json_file}: {e}")
                continue

    # Calculate aggregate statistics
    if stats["total_documents"] > 0:
        results = {
            "Total Documents Processed": stats["total_documents"],
            "Total Pages Processed": stats["total_pages"],
            "Total Blocks Extracted": stats["total_blocks"],
            "Average Pages per Document": statistics.mean(stats["pages_per_doc"]),
            "Average Blocks per Document": statistics.mean(stats["blocks_per_doc"]),
            "Average Blocks per Page": statistics.mean(stats["blocks_per_page"]),
            "Average Processing Time": f"{statistics.mean(stats['parsing_durations_ms']):.2f}ms",
            "Median Processing Time": f"{statistics.median(stats['parsing_durations_ms']):.2f}ms",
            "Pages per Second": stats["total_pages"] / processing_time_s,
            "Min Processing Time": f"{min(stats['parsing_durations_ms']):.2f}ms",
            "Max Processing Time": f"{max(stats['parsing_durations_ms']):.2f}ms",
        }

        # Print results in a formatted way
        print("\nParsing Statistics:")
        print("==================")
        for key, value in results.items():
            print(f"{key}: {value}")

        return results
    else:
        print("No valid documents found to analyze")
        return None


async def process_file(session, file_path, sem):
    """Process a single file using aiohttp with semaphore control."""
    filename = os.path.basename(file_path)

    async with sem:  # Use semaphore to limit concurrent requests
        try:
            # Prepare the file for upload
            data = aiohttp.FormData()
            data.add_field("file", open(file_path, "rb"), filename=filename)

            async with session.post(
                "http://localhost:3002/parse", data=data
            ) as response:
                if response.status == 200:
                    result = await response.text()
                    logger.info(f"Successfully processed: {filename}")
                    return filename, result
                else:
                    logger.error(
                        f"Error processing {filename}: Status {response.status}"
                    )
                    return filename, None
        except Exception as e:
            logger.error(f"Exception processing {filename}: {str(e)}")
            return filename, None


async def process_directory(
    input_dir, max_concurrent=4, output_dir="/tmp/pdf_responses", limit=None
):
    sem = Semaphore(max_concurrent)
    """Process all PDF files in the directory with concurrency limit."""
    input_path = Path(input_dir)
    pdf_files = list(input_path.glob("*.pdf"))
    if limit is not None:
        pdf_files = pdf_files[:limit]

    if not pdf_files:
        logger.warning(f"No PDF files found in {input_dir}")
        return

    # Create temporary directory for responses
    output_dir = Path(output_dir)
    output_dir.mkdir(exist_ok=True)
    logger.info(f"Storing responses in: {output_dir}")

    # Configure connection pooling
    async with aiohttp.ClientSession() as session:
        # Create tasks for all files, passing the semaphore
        tasks = [process_file(session, str(pdf), sem) for pdf in pdf_files]

        # Process files and gather results
        results = await asyncio.gather(*tasks)

        # Save results
        for filename, content in results:
            if content:
                output_file = output_dir / f"{filename}.json"
                with open(output_file, "w") as f:
                    f.write(content)


def main():
    parser = argparse.ArgumentParser(description="Process PDF files for parsing.")
    parser.add_argument("input_dir", help="Directory containing PDF files to process")
    parser.add_argument(
        "--max-concurrent",
        type=int,
        default=10,
        help="Maximum number of concurrent requests (default: 10)",
    )
    parser.add_argument(
        "--limit",
        type=int,
        help="Limit the number of PDF files to process (default: process all files)",
    )
    # Parse arguments
    args = parser.parse_args()

    # Verify input directory exists
    if not os.path.isdir(args.input_dir):
        logger.error(f"Input directory '{args.input_dir}' does not exist")
        return

    # Run the async process
    s = perf_counter()
    asyncio.run(
        process_directory(args.input_dir, args.max_concurrent, limit=args.limit)
    )
    logger.info("All files processed.")
    e = perf_counter()
    analyze_parsing_results(e - s)


if __name__ == "__main__":
    main()
